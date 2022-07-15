use async_std::task;
use eframe::egui::{self, RichText};
use futures::prelude::stream::StreamExt;
use futures::select;
use libp2p::{
    floodsub::{self, Floodsub, FloodsubEvent},
    identity,
    mdns::{Mdns, MdnsConfig, MdnsEvent},
    swarm::SwarmEvent,
    Multiaddr, NetworkBehaviour, PeerId, Swarm,
};
use std::sync::mpsc;

fn main() {
    // frontend to backend
    let (f_sender, f_reciever) = mpsc::channel();
    // backend to front
    let (b_sender, b_reciever) = mpsc::channel();
    std::thread::spawn(|| start(f_reciever, b_sender));
    eframe::run_native(
        "Gossip",
        eframe::NativeOptions::default(),
        Box::new(|_| Box::new(MyApp::new(b_reciever, f_sender))),
    );
}

/// used for internal communication from networking to frontend
enum PacketFromBackend {
    MessageRecieved((String, String)),
}

///used for internal communication from frontend to networking
enum PacketFromFrontend {
    SendMessage((String, String)),
}

struct Message {
    sender: String,
    text: String,
}

#[derive(Default)]
struct Chat {
    chat_name: String,
    messages: Vec<Message>,
}

struct MyApp {
    draft_text: String,
    chats: Vec<Chat>,
    chat_index: usize,
    reciever: mpsc::Receiver<PacketFromBackend>,
    sender: mpsc::Sender<PacketFromFrontend>,
}

impl MyApp {
    fn new(
        reciever: mpsc::Receiver<PacketFromBackend>,
        sender: mpsc::Sender<PacketFromFrontend>,
    ) -> Self {
        Self {
            draft_text: String::new(),
            chats: vec![
                Chat {
                    chat_name: String::from("new"),
                    messages: vec![Message {
                        sender: String::from("de"),
                        text: String::from("dejnjn"),
                    }],
                },
                Chat {
                    chat_name: String::from("news"),
                    ..Default::default()
                },
            ],
            chat_index: 0,
            sender,
            reciever,
        }
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Ok(PacketFromBackend::MessageRecieved((room, message))) = self.reciever.try_recv() {
            self.chats
                .iter_mut()
                .find(|x| x.chat_name == room)
                .unwrap()
                .messages
                .push(Message {
                    sender: room,
                    text: message,
                });
        }
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Gossip");
            ui.separator();
            ui.horizontal(|ui| {
                ui.horizontal(|ui| {
                    ui.set_height(ui.available_height());
                    ui.group(|ui| {
                        ui.vertical(|ui| {
                            ui.label("Your decentralised chats:   ");
                            for (i, chat) in self.chats.iter().enumerate() {
                                ui.add_space(10f32);
                                if ui
                                    .button(
                                        RichText::new(
                                            String::from("🌝  ") + chat.chat_name.as_str(),
                                        )
                                        .heading(),
                                    )
                                    .clicked()
                                {
                                    self.chat_index = i;
                                };
                                ui.add_space(10f32);
                            }
                        })
                    });
                    ui.group(|ui| {
                        ui.vertical(|ui| {
                            for message in &self.chats[self.chat_index].messages {
                                ui.group(|ui| {
                                    ui.label(RichText::new(&message.sender).heading());
                                    ui.label(&message.text);
                                });
                            }
                            ui.add_space(10f32);
                            ui.separator();
                            ui.horizontal(|ui| {
                                ui.text_edit_singleline(&mut self.draft_text);
                                if ui.button("Send").clicked() {
                                    self.sender
                                        .send(PacketFromFrontend::SendMessage((
                                            (&self.chats)[self.chat_index].chat_name.clone(),
                                            self.draft_text.clone(),
                                        )))
                                        .unwrap();
                                    //.unwrap_or(println!("Error sending!"));
                                    self.draft_text.clear();
                                }
                            })
                        })
                    })
                })
            });
        });
    }
}

//fn main() {
//   task::block_on(start());
//}

async fn start(
    reciever: mpsc::Receiver<PacketFromFrontend>,
    sender: mpsc::Sender<PacketFromBackend>,
) {
    // Create a random PeerId
    let local_key = identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_key.public());
    println!("Local peer id: {:?}", local_peer_id);

    // Set up an encrypted DNS-enabled TCP Transport over the Mplex and Yamux protocols
    let transport = libp2p::development_transport(local_key).await.unwrap();

    // Create a Floodsub topic
    let floodsub_topic = floodsub::Topic::new("chat");

    // We create a custom network behaviour that combines floodsub and mDNS.
    // In the future, we want to improve libp2p to make this easier to do.
    // Use the derive to generate delegating NetworkBehaviour impl and require the
    // NetworkBehaviourEventProcess implementations below.
    #[derive(NetworkBehaviour)]
    #[behaviour(out_event = "OutEvent")]
    struct MyBehaviour {
        floodsub: Floodsub,
        mdns: Mdns,

        // Struct fields which do not implement NetworkBehaviour need to be ignored
        #[behaviour(ignore)]
        #[allow(dead_code)]
        ignored_member: bool,
    }

    #[derive(Debug)]
    enum OutEvent {
        Floodsub(FloodsubEvent),
        Mdns(MdnsEvent),
    }

    impl From<MdnsEvent> for OutEvent {
        fn from(v: MdnsEvent) -> Self {
            Self::Mdns(v)
        }
    }

    impl From<FloodsubEvent> for OutEvent {
        fn from(v: FloodsubEvent) -> Self {
            Self::Floodsub(v)
        }
    }

    // Create a Swarm to manage peers and events
    let mut swarm = {
        let mdns = task::block_on(Mdns::new(MdnsConfig::default())).unwrap();
        let mut behaviour = MyBehaviour {
            floodsub: Floodsub::new(local_peer_id),
            mdns,
            ignored_member: false,
        };

        behaviour.floodsub.subscribe(floodsub_topic.clone());
        Swarm::new(transport, behaviour, local_peer_id)
    };

    // Listen on all interfaces and whatever port the OS assigns
    swarm
        .listen_on("/ip4/0.0.0.0/tcp/0".parse().unwrap())
        .unwrap();

    // Kick it off
    loop {
        if let Ok(PacketFromFrontend::SendMessage((reciever, message))) = reciever.try_recv() {
            swarm
                .behaviour_mut()
                .floodsub
                .publish(floodsub::Topic::new(reciever), message)
        }
        select! {
            event = swarm.select_next_some() => match event {
                SwarmEvent::NewListenAddr { address, .. } => {
                    println!("Listening on {:?}", address);
                }
                SwarmEvent::Behaviour(OutEvent::Floodsub(
                    FloodsubEvent::Message(message)
                )) => {
                    println!(
                        "Received: '{:?}' from {:?}",
                        String::from_utf8_lossy(&message.data),
                        message.source
                    );
                    sender.send(PacketFromBackend::MessageRecieved((String::from("de"),String::from("dff")))).unwrap();
                }
                SwarmEvent::Behaviour(OutEvent::Mdns(
                    MdnsEvent::Discovered(list)
                )) => {
                    for (peer, _) in list {
                        swarm
                            .behaviour_mut()
                            .floodsub
                            .add_node_to_partial_view(peer);
                    }
                }
                SwarmEvent::Behaviour(OutEvent::Mdns(MdnsEvent::Expired(
                    list
                ))) => {
                    for (peer, _) in list {
                        if !swarm.behaviour_mut().mdns.has_node(&peer) {
                            swarm
                                .behaviour_mut()
                                .floodsub
                                .remove_node_from_partial_view(&peer);
                        }
                    }
                },
                _ => {}
            }
        }
    }
}
