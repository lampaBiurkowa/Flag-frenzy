extern crate sfml;

use std::sync::Arc;
use std::time::Duration;

use sfml::audio::Music;
use sfml::graphics::{CircleShape, Color, Font, RectangleShape, RenderTarget, RenderWindow, Shape, Text, Transformable};
use sfml::system::Vector2f;
use sfml::window::mouse::Button;
use sfml::window::{ContextSettings, Event, Key, Style};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::Mutex;
use tokio::time::sleep;
use std::process::{Command as StdCommand, Child};

mod shared;
use crate::shared::*;
use tokio::net::TcpStream;

enum Scene { Menu, Game }

const FONT_PATH: &str = "font.ttf";
const MUSIC_PATH: &str = "bg.ogg";
const SERVER_CMD: &str = "./server";
const LOCAL_ADDR: &str = "127.0.0.1:54321";
const ADDR_FILE_PATH: &str = "addr.txt";
const GAME_TITLE: &str = "Flag Frenzy";

async fn read(mut reader: &mut Option<OwnedReadHalf>, game_state_clone: &Arc<Mutex<GameState>>) {
    let mut buffer: Vec<u8> = vec![0; 4096];
    loop {
        if let Some(r) = &mut reader {
            match (*r).read(&mut buffer).await {
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
    }
}

#[tokio::main]
async fn main() {
    let mut scene = Scene::Menu;
    let mut window = RenderWindow::new(
        (WINDOW_SIZE_X, WINDOW_SIZE_Y),
        GAME_TITLE,
        Style::CLOSE,
        &ContextSettings::default(),
    );
    window.set_vertical_sync_enabled(true);

    let font = Font::from_file(FONT_PATH).expect("Failed to load font");
    let mut bg_music = Music::from_file(MUSIC_PATH).expect("Failed to load background music");
    bg_music.set_looping(true);
    bg_music.play();

    let game_state = Arc::new(Mutex::new(GameState {
        players: vec![],
        flag_x: Default::default(),
        flag_y: Default::default(),
        flag_owner_id: Default::default(),
        bullets: vec![],
        boxes: vec![],
    }));

    let mut reader: Option<OwnedReadHalf>;
    let mut writer: Option<OwnedWriteHalf> = None;
    let mut player_id: u32 = 0; //hzd
    let mut server_process: Option<Child> = None;

    while window.is_open() {
        window.clear(Color::BLACK);

        match scene {
            Scene::Game => {
                let mut game_state_clone: GameState;
                { //critical section to get snapshot of game state
                    let game_state = game_state.lock().await;
                    game_state_clone = game_state.clone();
                }

                if let Some(w) = &mut writer {
                    while let Some(event) = window.poll_event() {
                        if event == Event::Closed {
                            window.close();
                        }
                        
                        handle_game_event(w, &game_state_clone, player_id, &event).await;
                    }
    
                    render_game(&mut window, w, &mut game_state_clone, player_id, &font).await;
                }
            }
            Scene::Menu => {
                while let Some(event) = window.poll_event() {
                    if event == Event::Closed {
                        window.close();
                    }

                    if let Event::KeyPressed { code, .. } = event {
                        match code {
                            Key::Num1 => {
                                match StdCommand::new(SERVER_CMD).spawn() {
                                    Ok(child) => {
                                        server_process = Some(child);
                                    },
                                    Err(e) => println!("{}", e)
                                }
                                sleep(Duration::from_millis(500)).await; // waitin for server to setup :D/
                                let (r, w) = connect(&mut player_id, true).await;
                                reader = Some(r);
                                writer = Some(w);
                                let game_state_clone = Arc::clone(&game_state);
                                tokio::spawn(async move {
                                        read(&mut reader, &game_state_clone).await;
                                    }
                                );
                                scene = Scene::Game;
                            },
                            Key::Num2 => {
                                let (r, w) = connect(&mut player_id, true).await;
                                reader = Some(r);
                                writer = Some(w);
                                let game_state_clone = Arc::clone(&game_state);
                                tokio::spawn(async move {
                                        read(&mut reader, &game_state_clone).await;
                                    }
                                );
                                scene = Scene::Game;
                            },
                            Key::Num3 => {      
                                if let Some(mut child) = server_process.take() {
                                    let _ = child.kill();
                                }
                                std::process::exit(0);  
                            },
                            _ => ()
                        }
                    }
                }

                render_menu(&mut window, &font);
            },
        }

        window.display();
    }

    if let Some(mut child) = server_process {
        let _ = child.kill();
    }

    ()
}

async fn connect(player_id: &mut u32, read_addr: bool) -> (OwnedReadHalf, OwnedWriteHalf) {
    let mut addr = String::from(LOCAL_ADDR);
    if read_addr {
        match std::fs::read_to_string(ADDR_FILE_PATH) {
            Ok(x) => addr = x,
            _ => ()
        }
    }
    let stream = TcpStream::connect(addr).await.unwrap();
    let (mut r, mut w) = stream.into_split();
    w.write_all(b"").await.unwrap();
    *player_id = r.read_u32().await.unwrap();

    return (r, w);
}

async fn handle_game_event(mut writer: &mut OwnedWriteHalf, game_state_clone: &GameState, player_id: u32, event: &Event) {
    if !game_state_clone.players.iter().any(|p| p.id == player_id) {
        return;
    }
    
    if let Event::MouseButtonPressed { button, x, y } = *event {
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

fn render_menu(window: &mut RenderWindow, font: &Font) {
    draw_centered_text(GAME_TITLE, 150.0, window, font, 50);
    draw_centered_text("1 - Play offline", 250.0, window, font, 30);
    draw_centered_text("2 - Play online", 300.0, window, font, 30);
    draw_centered_text("3 - Quit", 350.0, window, font, 30);
}

fn draw_centered_text(text: &str, y: f32, window: &mut RenderWindow, font: &Font, font_size: u32) {
    let mut quit_option = Text::new(text, font, font_size);
    quit_option.set_fill_color(Color::WHITE);
    quit_option.set_position((get_text_center_x(&quit_option), y));
    window.draw(&quit_option);
}

fn get_text_center_x(text: &Text) -> f32 {
    (WINDOW_SIZE_X as f32 - text.global_bounds().width) / 2.0
}

async fn render_game(window: &mut RenderWindow, mut writer: &mut OwnedWriteHalf, game_state_clone: &mut GameState, player_id: u32, font: &Font) {
    let player_option = game_state_clone.players.iter_mut().find(|p| p.id == player_id);
    let player;
    match player_option {
        None => return,
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

    for player in &game_state_clone.players {
        let mut circle = CircleShape::new(10.0, 30);
        circle.set_position(Vector2f::new(player.x , player.y));
        if player.has_flag {
            circle.set_fill_color(Color::RED);
        } else if player.id == player_clone.id {
            circle.set_fill_color(Color::CYAN)
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
        rect.set_size(Vector2f::new(BOX_SIZE, BOX_SIZE));
        rect.set_position(Vector2f::new(box_item.x, box_item.y));
        rect.set_fill_color(Color::YELLOW);
        window.draw(&rect);
    }

    let mut flag = CircleShape::new(FLAG_SIZE, 30);
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
}