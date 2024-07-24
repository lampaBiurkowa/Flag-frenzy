use std::sync::Arc;

use tokio::{net::tcp::OwnedWriteHalf, sync::Mutex};

use rand::Rng;

use crate::shared::*;

#[derive(Clone)]
pub struct Bot {
    player_id: u32,
    target_x: f32,
    target_y: f32,
}

impl Bot {
    pub fn new(player_id: u32) -> Self {
        Bot {
            player_id: player_id,
            target_x: rand::thread_rng().gen_range(0.0..WINDOW_SIZE_X as f32),
            target_y: rand::thread_rng().gen_range(0.0..WINDOW_SIZE_Y as f32),
        }
    }

    pub async fn run(&mut self, game_state: Arc<Mutex<GameState>>, writer: Arc<Mutex<OwnedWriteHalf>>) {
        let player = Player {
            id: self.player_id,
            x: 100.0,
            y: 100.0,
            has_flag: false,
            respawn_num: 0,
            score: 0,
        };

        send_command(&mut *writer.lock().await, CMD_PLAYER, &player).await;
        // {
        //     let mut game_state = game_state.lock().await;
        //     game_state.players.push(player);
        // }
        loop {
            let is_present;
            {
                is_present = game_state.lock().await.players.iter().any(|p| p.id == player.id);
            }
            
            if is_present {
                let processed_player = self.update(game_state.clone(), &writer).await;
                send_command(&mut *writer.lock().await, CMD_PLAYER, &processed_player).await;
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
    }

    async fn update(&mut self, game_state: Arc<Mutex<GameState>>, writer: &Arc<Mutex<OwnedWriteHalf>>) -> Player {
        let processed_player;
        let game_state_clone;
        {
            let mut game_state = game_state.lock().await;
            game_state_clone = game_state.clone();
            let flag_x = game_state.flag_x;
            let flag_y = game_state.flag_y;

            let player = game_state.players.iter_mut().find(|p| p.id == self.player_id).unwrap();
            if !player.has_flag {
                self.move_towards(flag_x, flag_y, player);
            } else {
                self.move_towards(self.target_x, self.target_y, player);
            }

            processed_player = player.clone();
        }

        for p in &game_state_clone.players {
            if p.id != processed_player.id {
                let distance = get_distance(processed_player.x, p.x, processed_player.y, p.y);
                if distance < 200.0 {
                    let (dx, dy) = normalize((p.x - processed_player.x, p.y - processed_player.y));
                    let bullet = Bullet {
                        x: processed_player.x,
                        y: processed_player.y,
                        dx,
                        dy,
                        owner_id: processed_player.id,
                    };
                    send_command(&mut *writer.lock().await, CMD_BULLET, &bullet).await;
                }
            }
        }

        processed_player
    }

    fn move_towards(&mut self, target_x: f32, target_y: f32, player: &mut Player) {
        let (dx, dy) = normalize((target_x - player.x, target_y - player.y));
        player.x += dx * 5.0;
        player.y += dy * 5.0;

        if get_distance(player.x, target_x, player.y, target_y) < 10.0 {
            self.target_x = rand::thread_rng().gen_range(0.0..WINDOW_SIZE_X as f32);
            self.target_y = rand::thread_rng().gen_range(0.0..WINDOW_SIZE_Y as f32);
        }
    }
}
