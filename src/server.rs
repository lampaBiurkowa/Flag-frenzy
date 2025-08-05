use rand::Rng;
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::{self, Instant};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::sync::Arc;
use tokio::sync::Mutex;
mod shared;
use crate::shared::*;
mod bot;
use crate::bot::Bot;

async fn handle_bot(bot_id: u32, game_state: Arc<Mutex<GameState>>) {
    let mut bot = Bot::new(bot_id);
    let bot_writer = Arc::new(Mutex::new(TcpStream::connect("0.0.0.0:32571").await.unwrap().into_split().1));
    bot.run(Arc::clone(&game_state), Arc::clone(&bot_writer)).await;
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("0.0.0.0:32571").await?;
    let mut player_id_counter = 1;

    let game_state = Arc::new(Mutex::new(GameState {
        players: vec![],
        flag_x: 400.0,
        flag_y: 300.0,
        flag_owner_id: Default::default(),
        bullets: vec![],
        boxes: vec![],
    }));
    initialize_boxes(&game_state).await;


    let clients = Arc::new(Mutex::new(HashMap::<u32, OwnedWriteHalf>::new()));
    tokio::spawn(send_data(Arc::clone(&clients), Arc::clone(&game_state)));

    let bot_count = fs::read_to_string("bots.txt").unwrap().trim().parse::<u32>().unwrap_or(0);
    for i in 0..bot_count {
        let bot_id = player_id_counter + i;
        tokio::spawn(handle_bot(bot_id, Arc::clone(&game_state)));
    }

    loop {
        let (socket, _addr) = listener.accept().await?;

        let game_state = Arc::clone(&game_state);
        let player_id = player_id_counter;
        println!("Connection established for {player_id}");
        player_id_counter += 1;

        let (reader, mut writer) = socket.into_split();
        if let Err(e) = writer.write_u32(player_id).await {
            println!("Failed to send player ID: {}", e);
        }

        {
            let mut clients = clients.lock().await;
            clients.insert(player_id, writer);
        }
        tokio::spawn(handle_connection(reader, game_state, player_id, Arc::clone(&clients)));
    }
}

async fn send_data(clients: Arc::<Mutex::<HashMap::<u32, OwnedWriteHalf>>>, game_state: Arc::<Mutex::<GameState>>)
{
    let mut captured_flag_timer = Instant::now();
    loop {
        let game_state_serialized;
        {
            let mut game_state = game_state.lock().await;
            let game_state_clone = game_state.clone();
            game_state_serialized = serde_json::to_string(&game_state_clone).unwrap();

            let bullet_count = game_state_clone.bullets.len();
            for i in (0..bullet_count).rev() { //rev for removing by index
                game_state.bullets[i].mov();

                let hit_box = game_state.boxes.iter().position(|b| get_distance(b.x, game_state_clone.bullets[i].x, b.y, game_state_clone.bullets[i].y) < BOX_SIZE);
                if let Some(index) = hit_box {
                    let (new_x, new_y) = find_free_spot(&game_state_clone);
                    game_state.boxes[index] = WoodBox { x: new_x, y: new_y };
                    game_state.bullets.remove(i); //won't crash because its descending :D/
                }
            }

            if let Some(player) = game_state_clone.players.iter().find(|p| get_distance(p.x, game_state_clone.flag_x, p.y, game_state_clone.flag_y) < 10.0) {
                game_state.flag_owner_id = Some(player.id);
                game_state.flag_x = player.x;
                game_state.flag_y = player.y;
                if !player.has_flag {
                    captured_flag_timer = Instant::now();
                    game_state.players.iter_mut().find(|p| p.id == player.id).unwrap().has_flag = true;
                }
            }

            if Instant::now() - captured_flag_timer > time::Duration::from_secs(1) {
                captured_flag_timer = Instant::now();
                if let Some(player) = game_state.players.iter_mut().find(|p| p.has_flag) {
                    player.score += 1;
                }
            }

            let mut shot_player_id = None;
            for player in &mut game_state.players {
                for bullet in &game_state_clone.bullets {
                    if bullet.owner_id != player.id &&
                        get_distance(player.x, bullet.x, player.y, bullet.y) < PLAYER_RADIUS {
                        player.respawn();
                        player.score -= 1;
                        player.has_flag = false;
                        shot_player_id = Some(bullet.owner_id);
                    }
                }
            }

            match shot_player_id
            {
                Some(x) => {
                    if let Some(shooter) = game_state.players.iter_mut().find(|p| p.id == x) {
                        shooter.score += 1;
                    }
                },
                _ => ()
            }

            game_state.bullets.retain(|b| b.x >= 0.0 && b.x <= WINDOW_SIZE_X as f32 && b.y >= 0.0 && b.y <= WINDOW_SIZE_Y as f32);
        }

        for c in clients.lock().await.iter_mut() {
            if let Err(e) = c.1.write_all(game_state_serialized.as_bytes()).await {
                println!("Failed to send game state: {}", e);
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(1000 / 24)).await;
    }
}

async fn handle_connection(
    mut reader: tokio::net::tcp::OwnedReadHalf,
    game_state: Arc<Mutex<GameState>>,
    player_id: u32,
    clients: Arc::<Mutex::<HashMap::<u32, OwnedWriteHalf>>>
) {
    let new_player = Player {
        id: player_id,
        x: 100.0,
        y: 100.0,
        has_flag: false,
        respawn_num: 0,
        score: 0
    };

    {
        let mut game_state = game_state.lock().await;
        game_state.players.push(new_player);
    }

    let read_game_state = Arc::clone(&game_state);
    let read_task = tokio::spawn(async move {
        let mut buffer = [0; 4096];
        loop {
            match reader.read(&mut buffer).await {
                Ok(n) => {
                    if n == 0 {
                        break;
                    }
                    
                    let data = &buffer[..n];
                    let data_str = match std::str::from_utf8(data) {
                        Ok(v) => v,
                        Err(_) => {
                            continue;
                        }
                    };

                    let separator_str = std::str::from_utf8(CMD_SEPARATOR).unwrap();
                    let parts: Vec<&str> = data_str.split(separator_str).collect();
                    let mut used_cmds = HashSet::<&[u8]>::new();
                    for part in parts {
                        let buffer = part.as_bytes();
                        let n = part.len();

                        if buffer.starts_with(CMD_PLAYER) && !used_cmds.contains(CMD_PLAYER)
                        {
                            used_cmds.insert(CMD_PLAYER);
                            handle_player_cmd(buffer, n, Arc::clone(&read_game_state)).await;
                        } else if buffer.starts_with(CMD_BULLET) && !used_cmds.contains(CMD_BULLET){
                            used_cmds.insert(CMD_BULLET);
                            handle_bullet_cmd(buffer, n, Arc::clone(&read_game_state)).await;
                        }
                    }
                },
                _ => ()
            };
        }

        let mut game_state = read_game_state.lock().await;
        game_state.players.retain(|p| p.id != player_id);
        clients.lock().await.remove(&player_id);
        println!("Player {} removed from the game state", player_id);
    });

    if let Err(e) = read_task.await {
        println!("Read task failed: {}", e);
    }
}

async fn handle_player_cmd(buffer: &[u8], n: usize, read_game_state: Arc::<Mutex::<GameState>>) {
    let result = serde_json::from_slice::<Player>(&buffer[CMD_PLAYER.len()..n]);
    match result {
        Ok(player) => {
            let mut game_state = read_game_state.lock().await;
            if let Some(current_player) = game_state.players.iter_mut().find(|p| p.id == player.id) {
                if player.respawn_num < current_player.respawn_num {
                    return;
                }

                current_player.x = player.x;
                current_player.y = player.y;
            }
        },
        Err(x)  => { println!("{x} {}", String::from_utf8_lossy(&buffer));}
    }
}

async fn handle_bullet_cmd(buffer: &[u8], n: usize, read_game_state: Arc::<Mutex::<GameState>>) {
    if let Ok(bullet) = serde_json::from_slice::<Bullet>(&buffer[CMD_BULLET.len()..n]) {
        let mut game_state = read_game_state.lock().await;
        game_state.bullets.push(bullet);
    }
}

async fn initialize_boxes(game_state: &Arc<Mutex<GameState>>) {
    let mut game_state = game_state.lock().await;
    for _ in 0..20 {
        let (x, y) = find_free_spot(&game_state);
        game_state.boxes.push(WoodBox { x, y });
    }
}

fn find_free_spot(game_state: &GameState) -> (f32, f32) {
    let mut rng = rand::thread_rng();
    loop {
        let x = rng.gen_range(0..WINDOW_SIZE_X - BOX_SIZE as u32);
        let y = rng.gen_range(0..WINDOW_SIZE_Y - BOX_SIZE as u32);
        if is_spot_free(x as f32, y as f32, game_state) {
            return (x as f32, y as f32);
        }
    }
}

fn is_spot_free(x: f32, y: f32, game_state: &GameState) -> bool {
    let slot_size = 20.0;
    let half_size = slot_size / 2.0;
    let occupied = game_state.players.iter().any(|p| {
        (p.x - x).abs() < half_size && (p.y - y).abs() < half_size
    }) || game_state.boxes.iter().any(|b| {
        (b.x - x).abs() < half_size && (b.y - y).abs() < half_size
    });
    !occupied
}
