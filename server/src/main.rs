use cgmath::{AbsDiffEq, Vector2};
use log::{error, info};
use raylib::consts::KeyboardKey;
use shared::constants::{
    BALL_RADIUS, BLOCKS_IN_ROW, BLOCK_SIZE, PADDLE_HEIGHT, PADDLE_WIDTH, WORLD_HEIGHT, WORLD_WIDTH,
};
use shared::world_data::{Ball, Block, Paddle, WorldData};
use std::error::Error;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::watch::Receiver;
use tokio::sync::{mpsc, watch};
use tracing::info_span;
use tracing::level_filters::LevelFilter;
use tracing::Instrument;
use tracing_subscriber::EnvFilter;
use watch::channel;
use wtransport::endpoint::IncomingSession;
use wtransport::ServerConfig;
use wtransport::{Endpoint, Identity};

const BLOCK_ROWS: usize = 5;
const BLOCK_HITS_LIFE: usize = 1;

const BALL_SPEED: usize = 300;

const PADDLE_SPEED: usize = 300;

const GAME_LOOP_TIMESTEP_SECONDS: f32 = 1.0 / 60.0;

struct PlayerKeyEvent {
    player_id: u8,
    key_code: u32,
}

#[tokio::main]
async fn main() {
    let (world_data_send_channel, world_data_receive_channel) = mpsc::unbounded_channel();

    let (player_key_event_send_channel, player_key_event_receive_channel) =
        mpsc::unbounded_channel();

    let game_loop_handle = tokio::spawn(async move {
        start_game_loop(world_data_send_channel, player_key_event_receive_channel).await
    });

    let server_handle = tokio::spawn(async move {
        start_server(world_data_receive_channel, player_key_event_send_channel).await
    });

    game_loop_handle.await.unwrap();
    server_handle.await.unwrap();
}

async fn start_game_loop(
    world_data_send_channel: mpsc::UnboundedSender<WorldData>,
    mut player_key_event_receive_channel: mpsc::UnboundedReceiver<PlayerKeyEvent>,
) {
    let mut world_data = create_world_data();

    loop {
        let mut paddles: [Paddle; 2] = world_data.paddles.clone();
        let mut balls: Vec<Ball> = world_data.balls.clone();

        while let Ok(event) = player_key_event_receive_channel.try_recv() {
            let index = paddles
                .iter()
                .position(|p| p.id == event.player_id)
                .unwrap();

            let mut paddle_to_move = paddles[index].clone();

            if event.key_code == KeyboardKey::KEY_LEFT as u32 {
                paddle_to_move.position.x -= PADDLE_SPEED as f32 * GAME_LOOP_TIMESTEP_SECONDS;
            }

            if event.key_code == KeyboardKey::KEY_RIGHT as u32 {
                paddle_to_move.position.x += PADDLE_SPEED as f32 * GAME_LOOP_TIMESTEP_SECONDS;
            }

            paddles[index] = paddle_to_move;

            if event.key_code == KeyboardKey::KEY_SPACE as u32 {
                let ball_index = balls.iter().position(|p| p.id == event.player_id).unwrap();
                let mut ball_to_move = balls[ball_index].clone();

                if !ball_to_move.is_free {
                    ball_to_move.velocity = Vector2::new(0.0, -1.0);
                    ball_to_move.is_free = true;
                    balls[ball_index] = ball_to_move;
                }
            }
        }

        for paddle in paddles.iter_mut() {
            if paddle.position.x - PADDLE_WIDTH as f32 / 2.0 <= 0.0 {
                paddle.position.x = PADDLE_WIDTH as f32 / 2.0;
            }

            if paddle.position.x + PADDLE_WIDTH as f32 / 2.0 >= WORLD_WIDTH as f32 {
                paddle.position.x = WORLD_WIDTH as f32 - PADDLE_WIDTH as f32 / 2.0;
            }
        }

        for ball in balls.iter_mut() {
            if (ball.position.x < 0.0 || ball.position.x.abs_diff_eq(&0.0, f32::EPSILON))
                || (ball.position.x + BALL_RADIUS as f32 > WORLD_WIDTH as f32
                    || ball
                        .position
                        .x
                        .abs_diff_eq(&(WORLD_WIDTH as f32), f32::EPSILON))
            {
                ball.velocity.x *= -1.0;
            }
        }

        balls.retain(|b| {
            (b.position.y <= 0.0) == false
                && (b.position.y + BALL_RADIUS as f32 >= WORLD_HEIGHT as f32) == false
        });

        for ball in balls.iter_mut() {
            for paddle in &paddles {
                if is_ball_collided_with_object(&ball, paddle.position, PADDLE_WIDTH, PADDLE_HEIGHT)
                {
                    let paddle_center = paddle.position.x;
                    let ball_center = ball.position.x;
                    let centers_difference = ball_center - paddle_center;

                    if !centers_difference.abs_diff_eq(&0.0, f32::EPSILON) {
                        let deflect_factor = centers_difference / (PADDLE_WIDTH as f32 / 2.0);
                        ball.velocity.x = deflect_factor;
                    }

                    ball.velocity.y *= -1.0;
                }
            }
        }

        let mut blocks: Vec<Block> = world_data.blocks.clone();

        for ball in balls.iter_mut() {
            for block in &mut blocks {
                if is_ball_collided_with_object(&ball, block.position, BLOCK_SIZE, BLOCK_SIZE) {
                    if is_ball_hit_top_or_bottom_of_block(&ball, &block) {
                        ball.velocity.y *= -1.0;
                    } else {
                        ball.velocity.x *= -1.0;
                    }

                    block.hits_life -= 1;

                    break;
                }
            }
        }

        blocks.retain(|b| b.hits_life != 0);

        for ball in balls.iter_mut() {
            if ball.is_free {
                ball.position += ball.velocity * BALL_SPEED as f32 * GAME_LOOP_TIMESTEP_SECONDS;
            }
        }

        world_data.blocks = blocks;
        world_data.paddles = paddles;
        world_data.balls = balls;

        world_data_send_channel.send(world_data.clone()).unwrap();

        tokio::time::sleep(Duration::from_secs_f32(GAME_LOOP_TIMESTEP_SECONDS)).await;
    }
}

fn create_world_data() -> WorldData {
    let mut blocks: Vec<Block> = vec![];

    for row_index in 0..BLOCK_ROWS {
        for block_index in 0..BLOCKS_IN_ROW {
            blocks.push(Block {
                position: Vector2::new(
                    (block_index * (BLOCK_SIZE + 1)) as f32 + (BLOCK_SIZE as f32 / 2.0),
                    (row_index * (BLOCK_SIZE + 1)) as f32
                        + (BLOCK_SIZE as f32 / 2.0)
                        + (WORLD_HEIGHT as f32 / 2.0)
                        - (BLOCK_SIZE as f32 * 2.0 + BLOCK_SIZE as f32 / 2.0),
                ),
                hits_life: BLOCK_HITS_LIFE,
            });
        }
    }

    let paddles: [Paddle; 2] = [
        Paddle {
            id: 1,
            position: Vector2::new(WORLD_WIDTH as f32 / 2.0, PADDLE_HEIGHT as f32),
        },
        Paddle {
            id: 0,
            position: Vector2::new(
                WORLD_WIDTH as f32 / 2.0,
                WORLD_HEIGHT as f32 - PADDLE_HEIGHT as f32,
            ),
        },
    ];

    let balls: Vec<Ball> = Vec::from([
        Ball {
            id: 1,
            position: Vector2::new(
                paddles[0].position.x,
                paddles[0].position.y + PADDLE_HEIGHT as f32 / 2.0 + BALL_RADIUS as f32,
            ),
            velocity: Vector2::new(0.0, 0.0),
            is_free: false,
        },
        Ball {
            id: 0,
            position: Vector2::new(
                paddles[1].position.x,
                paddles[1].position.y - PADDLE_HEIGHT as f32 / 2.0 - BALL_RADIUS as f32,
            ),
            velocity: Vector2::new(0.0, 0.0),
            is_free: false,
        },
    ]);

    WorldData {
        blocks,
        paddles,
        balls,
    }
}

async fn start_server(
    mut receive_channel: mpsc::UnboundedReceiver<WorldData>,
    player_key_event_send_channel: mpsc::UnboundedSender<PlayerKeyEvent>,
) {
    init_logging();

    let config = ServerConfig::builder()
        .with_bind_default(4433)
        .with_identity(&Identity::self_signed(&["localhost", "127.0.0.1", "::1"]).unwrap())
        .keep_alive_interval(Some(Duration::from_secs(3)))
        .build();

    let server = Endpoint::server(config).unwrap();

    info!("Server ready!");

    let initial_world_data = receive_channel.recv().await.unwrap();
    let (player_1_sender, player_1_receiver) = channel(initial_world_data.clone());
    let (player_2_sender, player_2_receiver) = channel(initial_world_data);

    tokio::spawn(async move {
        while let Some(data) = receive_channel.recv().await {
            let _ = player_1_sender.send(data.clone());
            let _ = player_2_sender.send(data);
        }
    });

    let incoming_session = server.accept().await;

    tokio::spawn(
        handle_connection(
            incoming_session,
            player_1_receiver,
            0,
            player_key_event_send_channel.clone(),
        )
        .instrument(info_span!("Player 0 connected!.")),
    );

    let incoming_session = server.accept().await;

    tokio::spawn(
        handle_connection(
            incoming_session,
            player_2_receiver,
            1,
            player_key_event_send_channel,
        )
        .instrument(info_span!("Player 1 connected!.")),
    );
}

fn init_logging() {
    let env_filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .from_env_lossy();

    tracing_subscriber::fmt()
        .with_target(true)
        .with_level(true)
        .with_env_filter(env_filter)
        .init();
}

async fn handle_connection(
    incoming_session: IncomingSession,
    receive_channel: Receiver<WorldData>,
    player_id: u8,
    player_key_event_send_channel: mpsc::UnboundedSender<PlayerKeyEvent>,
) {
    let result = handle_connection_impl(
        incoming_session,
        receive_channel,
        player_id,
        player_key_event_send_channel,
    )
    .await;
    error!("{:?}", result);
}

async fn handle_connection_impl(
    incoming_session: IncomingSession,
    mut receive_channel: Receiver<WorldData>,
    player_id: u8,
    player_key_event_send_channel: mpsc::UnboundedSender<PlayerKeyEvent>,
) -> Result<(), Box<dyn Error>> {
    info!("Waiting for session request...");

    let session_request = incoming_session.await?;

    info!(
        "New session: Authority: '{}', Path: '{}'",
        session_request.authority(),
        session_request.path()
    );

    let connection = session_request.accept().await?;

    let (mut send_stream, mut receive_stream) = connection.accept_bi().await?;
    send_stream.write_u8(player_id).await?;
    send_stream.flush().await?;

    loop {
        tokio::select! {
            player_key_sygnal = receive_stream.read_u32() => {
                player_key_event_send_channel.send(PlayerKeyEvent{player_id, key_code: player_key_sygnal?})?;
            }
            _ = receive_channel.changed() => {
                let world_data = receive_channel.borrow().clone();
                let buf = rmp_serde::to_vec(&world_data)?;
                let len = buf.len() as u32;
                send_stream.write_u32(len).await?;
                send_stream.write_all(&buf).await?;
                send_stream.flush().await?;
            }
        }
    }
}

fn is_ball_collided_with_object(
    ball: &Ball,
    position: Vector2<f32>,
    width: usize,
    height: usize,
) -> bool {
    let ball_left = ball.position.x - BALL_RADIUS as f32;
    let ball_right = ball.position.x + BALL_RADIUS as f32;
    let ball_top = ball.position.y - BALL_RADIUS as f32;
    let ball_bottom = ball.position.y + BALL_RADIUS as f32;

    let object_left = position.x - (width as f32 / 2.0);
    let object_right = position.x + (width as f32 / 2.0);
    let object_top = position.y - (height as f32 / 2.0);
    let object_bottom = position.y + (height as f32 / 2.0);

    ball_left < object_right
        && ball_right > object_left
        && ball_top < object_bottom
        && ball_bottom > object_top
}

fn is_ball_hit_top_or_bottom_of_block(ball: &Ball, block: &Block) -> bool {
    let vector_from_block_to_ball = ball.position - block.position;

    vector_from_block_to_ball.y.abs() > vector_from_block_to_ball.x.abs()
}
