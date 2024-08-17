use std::error::Error;
use std::hash::Hash;
use std::io::Read;
use std::task::{Context, Poll};
use std::time::Duration;
use futures::StreamExt;
use libp2p::{gossipsub, identity, mdns, Multiaddr, noise, PeerId, SwarmBuilder, tcp, yamux};
use libp2p::core::Endpoint;
use libp2p::core::transport::PortUse;
use libp2p::gossipsub::{Behaviour, Config};
use libp2p::identity::Keypair;
use libp2p::ping;
use libp2p::swarm::{ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour, SwarmEvent, THandler, THandlerInEvent, THandlerOutEvent, ToSwarm};
use tokio::{io, select};
use tokio::io::AsyncBufReadExt;
use log::{info, warn, debug, error};
// use serde_json::{Result, Value};
use serde::{Deserialize, Serialize};
use serde_json::Deserializer;

#[derive(NetworkBehaviour)]
struct MyBehaviour {
    gossipsub: Behaviour,
    mdns: mdns::tokio::Behaviour,
    // pingBehaviour: ping::Behaviour,
}

#[derive(Serialize, Deserialize, Debug)]
struct MyMessage {
    message: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();
    // Create a key pair (unique identifier for a node)
    let local_keypair = createNewKeyPair();

    // This is the topic that the nodes will subscribe to
    // TODO: use the public key for the topic name
    let topic = gossipsub::IdentTopic::new("test-net");

    let mdns = mdns::tokio::Behaviour::new(mdns::Config::default(), local_keypair.public().to_peer_id())?;

    let gossipsub = Behaviour::new(
        gossipsub::MessageAuthenticity::Signed(local_keypair.clone()),
        get_gossipsub_config()?,
    )?;

    let pingBehaviour = ping::Behaviour::new(ping::Config::default());

    let mut swarm = SwarmBuilder::with_existing_identity(local_keypair.clone())
        .with_tokio()
        .with_quic()
        .with_behaviour(|_local_keypair| {
            Ok(MyBehaviour { gossipsub, mdns })
        })?
        .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
        .build();

    // subscribes to our topic
    swarm.behaviour_mut().gossipsub.subscribe(&topic)?;
    swarm.listen_on("/ip4/0.0.0.0/udp/0/quic-v1".parse()?)?;
    println!("Enter messages via STDIN and they will be sent to connected peers using Gossipsub");

    let mut stdin = io::BufReader::new(io::stdin()).lines();
    loop {
        select! {
            Ok(Some(line)) = stdin.next_line() =>{
                println!("{}",line);
                let message = MyMessage{
                    message: line
                };
                let messageStr = serde_json::to_string(&message)?;
                if let Err(e) = swarm.behaviour_mut().gossipsub.publish(topic.clone(), messageStr.as_bytes()) {
                    error!("Publish error: {e:?}");
                }
            }
            //TODO: What is this MyBehaviourEvent?
            event = swarm.select_next_some() => match event {
                SwarmEvent::Behaviour(MyBehaviourEvent::Mdns(mdns::Event::Discovered(list))) => {
                    for (peer_id, _multiaddr) in list {
                        debug!("mDNS discovered a new peer: {peer_id}");
                        swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                    }
                },
                SwarmEvent::Behaviour(MyBehaviourEvent::Mdns(mdns::Event::Expired(list))) => {
                    for (peer_id, _multiaddr) in list {
                        debug!("mDNS discover peer has expired: {peer_id}");
                        swarm.behaviour_mut().gossipsub.remove_explicit_peer(&peer_id);
                    }
                },
                SwarmEvent::NewListenAddr { address, .. } => {
                    debug!("Local node is listening on {address}");
                },
                SwarmEvent::Behaviour(MyBehaviourEvent::Gossipsub(gossipsub::Event::Message {
                    propagation_source: peer_id,
                    message_id: id,
                    message })) => {
                        let message_struct:MyMessage = serde_json::from_slice(&message.data)?;
                        info!(
                            "Got message: '{}' with id: {id} from peer: {peer_id}",
                            &message_struct.message,
                        );

                        // serde_json::from_str(message.data)from_str(&message.data).unwrap().
                    }

                _ => {}
            }
        }
    }

    // Ok(()).expect("TODO: panic message");
}

fn get_gossipsub_config() -> Result<Config, Box<dyn Error>> {
    let gossipsub_config = gossipsub::ConfigBuilder::default()
        .heartbeat_interval(Duration::from_secs(10)) // This is set to aid debugging by not cluttering the log space
        .validation_mode(gossipsub::ValidationMode::Strict) // This sets the kind of message validation. The default is Strict (enforce message signing)
        .build()
        .map_err(|msg| io::Error::new(io::ErrorKind::Other, msg))?;
    Ok(gossipsub_config)
}

fn createNewKeyPair() -> Keypair {
    let local_keypair: Keypair = Keypair::generate_ed25519();
    let local_peer_id: PeerId = PeerId::from(local_keypair.public());
    println!("Local peer id: {:?}", local_peer_id);

    local_keypair
}

