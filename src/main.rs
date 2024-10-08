mod schema;

use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::time::Duration;
use futures::StreamExt;
use libp2p::{gossipsub, mdns, PeerId, SwarmBuilder};
use libp2p::gossipsub::{Behaviour, Config};
use libp2p::identity::Keypair;
use libp2p::swarm::{NetworkBehaviour, SwarmEvent};
use tokio::{io, select};
use tokio::io::AsyncBufReadExt;
use log::{info, debug, error};

use crate::schema::message::InputMessage;


#[derive(NetworkBehaviour)]
struct MyBehaviour {
    gossipsub: Behaviour,
    mdns: mdns::tokio::Behaviour,
    // pingBehaviour: ping::Behaviour,
}

#[tokio::main]
//dyn dynamic error
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();
    let args: Vec<String> = env::args().collect();

    let topic_name = &args[1];

    // Create a key pair (unique identifier for a node)
    let local_keypair = create_new_key_pair();

    // This is the topic that the nodes will subscribe to
    // TODO: use the public key for the topic name
    let mdns = mdns::tokio::Behaviour::new(mdns::Config::default(), local_keypair.public().to_peer_id())?;

    let gossipsub = Behaviour::new(
        gossipsub::MessageAuthenticity::Signed(local_keypair.clone()),
        get_gossipsub_config()?,
    )?;

    // let ping_behaviour = ping::Behaviour::new(ping::Config::default());

    let mut swarm = SwarmBuilder::with_existing_identity(local_keypair.clone())
        .with_tokio()
        .with_quic()
        .with_behaviour(|_local_keypair| {
            Ok(MyBehaviour { gossipsub, mdns })
        })?
        .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
        .build();

    // subscribes to our topic
    swarm.listen_on("/ip4/0.0.0.0/udp/0/quic-v1".parse()?)?;
    println!("Enter messages via STDIN and they will be sent to connected peers using Gossipsub");
    let mut stdin = io::BufReader::new(io::stdin()).lines();
    let topic = gossipsub::IdentTopic::new(topic_name);
    swarm.behaviour_mut().gossipsub.subscribe(&topic)?;

    // clip message hashmap to store list of messages from all the devices
    let mut device_messages: HashMap<String, Vec<String>> = HashMap::new();


    loop {
        select! {
            Ok(Some(line)) = stdin.next_line() => {
                        println!("{}",line);
                        let message = InputMessage::new(line);
                        println!("{:?}", message);
                        publish_message(topic_name, &message, &mut swarm.behaviour_mut().gossipsub);
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
                        let message_struct:InputMessage = serde_json::from_slice(&message.data)?;
                        device_messages.entry(peer_id.to_string()).or_insert(Vec::new()).push(message_struct.message.clone());

                        info!(
                            "Got message: '{}' and timestamp: '{}'\n id: {id} \n peer: {peer_id}",
                            &message_struct.message, &message_struct.timestamp
                        );
                        info!(
                            "Device Messages: {:?}", device_messages
                        );

                    }

                _ => {}
            }
        }
    }

}
fn publish_message(topic_name: &String, message: &InputMessage, gossip_behaviour: &mut libp2p::gossipsub::Behaviour) -> bool {
    let topic = gossipsub::IdentTopic::new(topic_name);
    let message_str = serde_json::to_string(&message).unwrap();
    if let Err(e) = gossip_behaviour.publish(topic.clone(), message_str.as_bytes()) {
        error!("Publish error: {e:?}");
        false;
    }
    true
}

fn get_gossipsub_config() -> Result<Config, Box<dyn Error>> {
    let gossipsub_config = gossipsub::ConfigBuilder::default()
        .heartbeat_interval(Duration::from_secs(10)) // This is set to aid debugging by not cluttering the log space
        .validation_mode(gossipsub::ValidationMode::Strict) // This sets the kind of message validation. The default is Strict (enforce message signing)
        .build()
        .map_err(|msg| io::Error::new(io::ErrorKind::Other, msg))?;
    Ok(gossipsub_config)
}

fn create_new_key_pair() -> Keypair {
    let local_keypair: Keypair = Keypair::generate_ed25519();
    let local_peer_id: PeerId = PeerId::from(local_keypair.public());
    println!("Local peer id: {:?}", local_peer_id);

    local_keypair
}

