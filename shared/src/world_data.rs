use cgmath::Vector2;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug)]
pub struct WorldData {
    pub blocks: Vec<Block>,
    pub paddles: [Paddle; 2],
    pub balls: Vec<Ball>,
}

impl Clone for WorldData {
    fn clone(&self) -> Self {
        WorldData {
            blocks: self.blocks.clone(),
            paddles: self.paddles.clone(),
            balls: self.balls.clone(),
        }
    }
}

#[derive(Deserialize, Serialize, Debug)]
pub struct Block {
    pub position: Vector2<f32>,
    pub hits_life: usize,
}

impl Clone for Block {
    fn clone(&self) -> Self {
        Block {
            position: self.position,
            hits_life: self.hits_life,
        }
    }
}

#[derive(Deserialize, Serialize, Debug)]
pub struct Paddle {
    pub id: u8,
    pub position: Vector2<f32>,
}

impl Clone for Paddle {
    fn clone(&self) -> Self {
        Paddle {
            id: self.id,
            position: self.position,
        }
    }
}

#[derive(Deserialize, Serialize, Debug)]
pub struct Ball {
    pub id: u8,
    pub position: Vector2<f32>,
    pub velocity: Vector2<f32>,
    pub is_free: bool,
}

impl Clone for Ball {
    fn clone(&self) -> Self {
        Ball {
            id: self.id,
            position: self.position,
            velocity: self.velocity,
            is_free: self.is_free,
        }
    }
}
