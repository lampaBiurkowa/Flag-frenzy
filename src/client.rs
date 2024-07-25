extern crate sfml;

use std::sync::Arc;

use sfml::audio::Music;
use sfml::graphics::{CircleShape, Color, Font, RectangleShape, RenderTarget, RenderWindow, Shape, Text, Transformable};
use sfml::system::Vector2f;
use sfml::window::mouse::Button;
use sfml::window::{ContextSettings, Event, Key, Style};
use shared::{get_distance, normalize, send_command, Bullet, CMD_BULLET, CMD_PLAYER, PLAYER_RADIUS, WINDOW_SIZE_X, WINDOW_SIZE_Y};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex;

mod shared;
use crate::shared::GameState;
use tokio::net::TcpStream;

fn get_text_center_x(text: &Text) -> f32 {
    (WINDOW_SIZE_X as f32 - text.global_bounds().width) / 2.0
}

#[tokio::main]
async fn main() {
    let mut window = RenderWindow::new(
        (WINDOW_SIZE_X, WINDOW_SIZE_Y),
        "jakis ctf",
        Style::CLOSE,
        &ContextSettings::default(),
    );
    window.set_vertical_sync_enabled(true);

    let font = Font::from_file("font.ttf").expect("Failed to load font");

    let mut bg_music = Music::from_file("bg.ogg").expect("Failed to load background music");
    bg_music.set_looping(true);
    bg_music.play();

    let stream = TcpStream::connect("127.0.0.1:54321").await.unwrap();
    let (mut reader, mut writer) = stream.into_split();
    let mut buffer = vec![0; 4096];

    let game_state = Arc::new(Mutex::new(GameState {
        players: vec![],
        flag_x: Default::default(),
        flag_y: Default::default(),
        flag_owner_id: Default::default(),
        bullets: vec![],
        boxes: vec![],
    }));
    let game_state_clone = Arc::clone(&game_state);

    writer.write_all(b"").await.unwrap();
    let player_id = reader.read_u32().await.unwrap();
    tokio::spawn(async move {
        loop {
            match reader.read(&mut buffer).await {
                Ok(n) if n > 0 => {
                    let response = serde_json::from_slice::<GameState>(&buffer[..n]);
                    match response {
                        Ok(x) => {
                            let mut game_state = game_state_clone.lock().await;
                            *game_state = x;
                        },
                        _ => {
                            println!("Invalid packet received, ignoring {}", std::str::from_utf8(&buffer).unwrap());
                        }
                    }
                }
                _ => {
                    break;
                },
            }
        }
    });

    while window.is_open() {
        let mut game_state_clone: GameState;
        { //critical section to get snapshot of game state
            let game_state = game_state.lock().await;
            game_state_clone = game_state.clone();
        }

        while let Some(event) = window.poll_event() {
            if event == Event::Closed {
                window.close();
            }

            if !game_state_clone.players.iter().any(|p| p.id == player_id) {
                continue;
            }
            
            if let Event::MouseButtonPressed { button, x, y } = event {
                if button == Button::Left {
                    let player = game_state_clone.players.iter().find(|p| p.id == player_id).unwrap();
                    let (dx, dy) = normalize(((x as f32 - player.x), (y as f32 - player.y)));
                    let bullet = Bullet {
                        x: player.x,
                        y: player.y,
                        dx,
                        dy,
                        owner_id: player.id,
                    };
                    
                    send_command(&mut writer, CMD_BULLET, &bullet).await;
                }
            }
        }
        
        let player_option = game_state_clone.players.iter_mut().find(|p| p.id == player_id);
        let player;
        match player_option {
            None => continue,
            _ => player = player_option.unwrap()
        }
        let player_clone = player.clone(); //😐

        let mut dx = 0.0;
        let mut dy = 0.0;
        if player.y > 0.0 && Key::is_pressed(Key::W) {
            dy = -5.0;
        }
        else if player.y < WINDOW_SIZE_Y as f32 - PLAYER_RADIUS && Key::is_pressed(Key::S) {
            dy = 5.0;
        }
        if player.x > 0.0 && Key::is_pressed(Key::A) {
            dx = -5.0;
        }
        else if player.x < WINDOW_SIZE_X as f32 - PLAYER_RADIUS && Key::is_pressed(Key::D) {
            dx = 5.0;
        }
        
        player.x += dx;
        player.y += dy;
        for box_item in &game_state_clone.boxes {
            if get_distance(player.x, box_item.x, player.y, box_item.y) < PLAYER_RADIUS {
                player.x -= dx;
                player.y -= dy;
            }
        }

        send_command(&mut writer, CMD_PLAYER, &player).await;
        window.clear(Color::BLACK);

        for player in &game_state_clone.players {
            let mut circle = CircleShape::new(10.0, 30);
            circle.set_position(Vector2f::new(player.x , player.y));
            if player.has_flag {
                circle.set_fill_color(Color::RED);
            } else {
                circle.set_fill_color(Color::GREEN);
            }
            window.draw(&circle);
        }

        for bullet in &game_state_clone.bullets {
            let mut circle = CircleShape::new(5.0, 30);
            circle.set_position(Vector2f::new(bullet.x , bullet.y));
            circle.set_fill_color(Color::MAGENTA);
            window.draw(&circle);
        }
        
        for box_item in &game_state_clone.boxes {
            let mut rect = RectangleShape::new();
            rect.set_size(Vector2f::new(20.0, 20.0));
            rect.set_position(Vector2f::new(box_item.x, box_item.y));
            rect.set_fill_color(Color::YELLOW);
            window.draw(&rect);
        }

        let mut flag = CircleShape::new(5.0, 30);
        flag.set_position(Vector2f::new(game_state_clone.flag_x, game_state_clone.flag_y));
        flag.set_fill_color(Color::BLUE);
        window.draw(&flag);

        let mut player_score_text = Text::new(&format!("You ({}): {}",player_clone.id, player_clone.score), &font, 16);
        player_score_text.set_fill_color(Color::WHITE);
        player_score_text.set_position((20.0, 20.0));
        window.draw(&player_score_text);

        let column_height = 20.0;
        for (index, p) in game_state_clone.players.iter().enumerate() {
            let mut player_score_text = Text::new(&format!("{}: {}",p.id, p.score), &font, 16);
            player_score_text.set_fill_color(Color::WHITE);
            player_score_text.set_position((20.0, 20.0 + (column_height * (index + 1) as f32)));
            window.draw(&player_score_text);
        }

        window.display();
    }

    ()
}