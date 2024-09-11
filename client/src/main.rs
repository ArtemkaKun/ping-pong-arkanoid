use cgmath::Vector2;
use raylib::color::Color;
use raylib::consts::KeyboardKey;
use raylib::drawing::RaylibDraw;
use raylib::init;
use shared::constants::{
    BALL_RADIUS, BLOCK_SIZE, PADDLE_HEIGHT, PADDLE_WIDTH, WORLD_HEIGHT, WORLD_WIDTH,
};
use shared::world_data::WorldData;
use std::error::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use wtransport::Endpoint;
use wtransport::{ClientConfig, RecvStream, SendStream};

#[tokio::main]
async fn main() {
    let config = ClientConfig::builder()
        .with_bind_default()
        .with_no_cert_validation()
        .build();

    let connection = Endpoint::client(config)
        .unwrap()
        .connect("https://localhost:4433")
        .await
        .unwrap();

    let (send_stream, receive_stream) = connection.open_bi().await.unwrap().await.unwrap();
    start_game_loop(send_stream, receive_stream).await.unwrap();
}

async fn start_game_loop(
    mut send_stream: SendStream,
    mut receive_stream: RecvStream,
) -> Result<(), Box<dyn Error>> {
    let player_id = receive_stream.read_u8().await?;
    println!("Connected as Player {}", player_id);

    let mut world_data: WorldData;

    loop {
        match read_world_data(&mut receive_stream).await {
            Ok(Some(data)) => {
                world_data = data;
                break;
            }
            _ => continue,
        }
    }

    let (mut handle, thread) = init()
        .size(WORLD_WIDTH as i32, WORLD_HEIGHT as i32)
        .title("Ping Pong Arkanoid")
        .vsync()
        .build();

    while !handle.window_should_close() {
        if handle.is_key_down(KeyboardKey::KEY_SPACE) {
            send_stream.write_u32(KeyboardKey::KEY_SPACE as u32).await?;
            send_stream.flush().await?;
        }

        if handle.is_key_down(KeyboardKey::KEY_LEFT) {
            send_stream.write_u32(KeyboardKey::KEY_LEFT as u32).await?;
            send_stream.flush().await?;
        }

        if handle.is_key_down(KeyboardKey::KEY_RIGHT) {
            send_stream.write_u32(KeyboardKey::KEY_RIGHT as u32).await?;
            send_stream.flush().await?;
        }

        match read_world_data(&mut receive_stream).await {
            Ok(Some(data)) => {
                world_data = data;
            }
            Ok(None) => {
                // No data available, continue with old data
            }
            Err(e) => {
                eprintln!("Error reading WorldData: {:?}", e);
                // Handle error, maybe break loop or continue
            }
        }

        let mut draw_handle = handle.begin_drawing(&thread);

        draw_handle.clear_background(Color::from_hex("FFF4EA").unwrap());

        for block in world_data.blocks.clone() {
            let block_position = if player_id == 1 {
                rotate_180_around_world_center(block.position)
            } else {
                block.position
            };

            draw_handle.draw_rectangle(
                block_position.x as i32 - (BLOCK_SIZE as i32 / 2),
                block_position.y as i32 - (BLOCK_SIZE as i32 / 2),
                BLOCK_SIZE as i32,
                BLOCK_SIZE as i32,
                Color::from_hex("7EACB5").unwrap(),
            );
        }

        for paddle in world_data.paddles.clone() {
            let paddle_position = if player_id == 1 {
                rotate_180_around_world_center(paddle.position)
            } else {
                paddle.position
            };

            let paddle_color = if paddle.id == 0 {
                Color::from_hex("FADFA1").unwrap()
            } else {
                Color::from_hex("6A9C89").unwrap()
            };

            draw_handle.draw_rectangle(
                paddle_position.x as i32 - (PADDLE_WIDTH as i32 / 2),
                paddle_position.y as i32 - (PADDLE_HEIGHT as i32 / 2),
                PADDLE_WIDTH as i32,
                PADDLE_HEIGHT as i32,
                paddle_color,
            );
        }

        for ball in world_data.balls.clone() {
            let ball_position = if player_id == 1 {
                rotate_180_around_world_center(ball.position)
            } else {
                ball.position
            };

            draw_handle.draw_circle(
                ball_position.x as i32,
                ball_position.y as i32,
                BALL_RADIUS as f32,
                Color::from_hex("C96868").unwrap(),
            );
        }
    }

    Ok(())
}

async fn read_world_data(stream: &mut RecvStream) -> Result<Option<WorldData>, Box<dyn Error>> {
    let len = match stream.read_u32().await {
        Ok(len) => len,
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return Ok(None),
        Err(e) => return Err(Box::new(e)),
    };

    let mut buffer = vec![0; len as usize];
    stream.read_exact(&mut buffer).await?;

    let data = rmp_serde::from_slice(&buffer)?;
    Ok(Some(data))
}

fn rotate_180_around_world_center(vector: Vector2<f32>) -> Vector2<f32> {
    let world_center = Vector2::new(WORLD_WIDTH as f32 / 2.0, WORLD_HEIGHT as f32 / 2.0);
    let translated = vector - world_center;
    let rotated = Vector2::new(-translated.x, -translated.y);
    world_center + rotated
}
