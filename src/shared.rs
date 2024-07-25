use serde::{Deserialize, Serialize};
use rand::Rng;
use tokio::{io::AsyncWriteExt, net::tcp::OwnedWriteHalf};

pub(crate) const WINDOW_SIZE_X: u32 = 800;
pub(crate) const WINDOW_SIZE_Y: u32 = 600;
pub(crate) const PLAYER_RADIUS: f32 = 20.0;
pub(crate) const BOX_SIZE: f32 = 20.0;
pub(crate) const CMD_PLAYER: &[u8] = b"PLAYER";
pub(crate) const CMD_BULLET: &[u8] = b"BULLET";
pub(crate) const CMD_SEPARATOR: &[u8] = b":D/";

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GameState {
    pub players: Vec<Player>,
    pub flag_x: f32,
    pub flag_y: f32,
    pub flag_owner_id: Option<u32>,
    pub bullets: Vec<Bullet>,
    pub boxes: Vec<WoodBox>
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Player {
    pub id: u32,
    pub x: f32,
    pub y: f32,
    pub has_flag: bool,
    pub respawn_num: u32,
    pub score: i32
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Bullet {
    pub x: f32,
    pub y: f32,
    pub dx: f32,
    pub dy: f32,
    pub owner_id: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WoodBox {
    pub x: f32,
    pub y: f32,
}

impl Player {
    pub fn respawn(&mut self) {
        let mut rng = rand::thread_rng();
        self.x = rng.gen_range(0..=WINDOW_SIZE_X - PLAYER_RADIUS as u32) as f32;
        self.y = rng.gen_range(0..=WINDOW_SIZE_Y - PLAYER_RADIUS as u32) as f32;
        self.respawn_num += 1;
    }
}

impl Bullet {
    const SPEED: f32 = 20.0;

    pub fn mov(&mut self) {
        self.x += self.dx * Bullet::SPEED;
        self.y += self.dy * Bullet::SPEED;
    }
}

pub fn normalize(vec: (f32, f32)) -> (f32, f32) {
    let magnitude = (vec.0 * vec.0 + vec.1 * vec.1).sqrt();
    if magnitude == 0.0 {
        (0.0, 0.0)
    } else {
        (vec.0 / magnitude, vec.1 / magnitude)
    }
}

pub async fn send_command<T>(writer: &mut OwnedWriteHalf, cmd: &[u8], obj: &T) where T: Serialize {
    let mut message = Vec::new();
    message.extend_from_slice(cmd);
    message.extend_from_slice(serde_json::to_string(obj).unwrap().as_bytes());
    message.extend_from_slice(CMD_SEPARATOR);
    writer.write_all(&message).await.unwrap();
}

pub fn get_distance(x1: f32, x2: f32, y1: f32, y2: f32) -> f32 {
    f32::sqrt((x1 - x2).powi(2) + (y1 - y2).powi(2))
}
