use std::{sync::Arc, time};
use tokio::{net::tcp::OwnedWriteHalf, sync::Mutex, time::Instant};
use rand::Rng;

use crate::shared::*;

#[derive(Clone)]
pub struct Bot {
    player_id: u32,
    target_x: f32,
    target_y: f32,
    last_shot_time: Instant
}

impl Bot {
    const SHOOT_FREQ_MILLIS: u64 = 400;

    pub fn new(player_id: u32) -> Self {
        Bot {
            player_id: player_id,
            target_x: rand::thread_rng().gen_range(0.0..WINDOW_SIZE_X as f32),
            target_y: rand::thread_rng().gen_range(0.0..WINDOW_SIZE_Y as f32),
            last_shot_time: Instant::now()
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

        if !self.can_shoot() {
            return processed_player;
        }

        for box_item in &game_state_clone.boxes {
            if self.is_box_blocking_path(&processed_player, box_item) {
                let (dx, dy) = normalize((box_item.x - processed_player.x, box_item.y - processed_player.y));
                self.shoot(&processed_player, dx, dy, writer).await;
            }
        }

        for p in &game_state_clone.players {
            if p.id != processed_player.id {
                let distance = get_distance(processed_player.x, p.x, processed_player.y, p.y);
                if distance < 200.0 {
                    let (dx, dy) = normalize((p.x - processed_player.x, p.y - processed_player.y));
                    self.shoot(&processed_player, dx, dy, writer).await;
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

    async fn shoot(&mut self, player: &Player, dx: f32, dy: f32, writer: &Arc<Mutex<OwnedWriteHalf>>) {
        let bullet = Bullet {
            x: player.x,
            y: player.y,
            dx,
            dy,
            owner_id: player.id,
        };
        if self.can_shoot() {
            send_command(&mut *writer.lock().await, CMD_BULLET, &bullet).await;
            self.last_shot_time = Instant::now();
        }
    }

    fn can_shoot(&mut self) -> bool {
        Instant::now() - self.last_shot_time > time::Duration::from_millis(Bot::SHOOT_FREQ_MILLIS)
    }

    fn is_box_blocking_path(&self, player: &Player, box_item: &WoodBox) -> bool {
        let bot_to_target_distance = get_distance(player.x, self.target_x, player.y, self.target_y);
        let bot_to_box_distance = get_distance(player.x, box_item.x, player.y, box_item.y);
        let box_to_target_distance = get_distance(box_item.x, self.target_x, box_item.y, self.target_y);

        bot_to_box_distance < 50.0 && bot_to_box_distance + box_to_target_distance < bot_to_target_distance + 10.0 // Allow a small margin of error
    }
}
