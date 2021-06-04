#![allow(unreachable_code)]

use gossip_async::gossip::Gossip;

use async_std::channel::unbounded;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EventData {
    data: String,
    count: usize,
}

#[async_std::main]
async fn main() -> Result<(), anyhow::Error> {
    env_logger::init();

    let args: Vec<String> = std::env::args()
        .skip(1)
        .map(|n| format!("{}:{}", n, 8000))
        .collect();

    if args.len() < 2 {
        panic!("Usage <Port> <Seed nodes...>");
    }

    let address = args[0].clone();
    let seed_nodes = &args[1..];

    let gossip = Gossip::new(address, seed_nodes).await;

    let (event_sender, event_receiver) = unbounded();

    // futures::join!(gossip.await.initialize());

    futures::try_join!(
        gossip.start::<EventData>(event_sender),
        gossip.handler(event_receiver),
    )
    .unwrap();

    Ok(())
}
