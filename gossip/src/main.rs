use eframe::egui::style::Margin;
use eframe::egui::{self, Button, RichText, TextEdit};
use eframe::epaint::Color32;
use futures::channel::mpsc;
use futures::prelude::stream::StreamExt;
use futures::select;

use libp2p::gossipsub::MessageId;
use libp2p::gossipsub::{
    GossipsubEvent, GossipsubMessage, IdentTopic as Topic, MessageAuthenticity, ValidationMode,
};
use libp2p::{gossipsub, identity, swarm::SwarmEvent, PeerId};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::Duration;

fn main() {
    // frontend to backend
    let (mut f_sender, mut f_reciever) = mpsc::channel(128);
    // backend to front
    let (mut b_sender, mut b_reciever) = mpsc::channel(128);
    std::thread::spawn(move || async_std::task::block_on(start(&mut f_reciever, &mut b_sender)));
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
    AddPeer(String),
}

struct Message {
    sender: String,
    text: String,
}

#[derive(Default)]
struct Chat {
    chat_name: String,
    chat_peer_id: String,
    messages: Vec<Message>,
}

struct MyApp {
    draft_text: String,
    add_peer_text: String,
    chats: Vec<Chat>,
    chat_index: usize,
    reciever: mpsc::Receiver<PacketFromBackend>,
    frame: egui::Frame,
    sender: mpsc::Sender<PacketFromFrontend>,
}

impl MyApp {
    fn new(
        reciever: mpsc::Receiver<PacketFromBackend>,
        sender: mpsc::Sender<PacketFromFrontend>,
    ) -> Self {
        Self {
            draft_text: String::new(),
            chats: Vec::new(),
            chat_index: 0,
            sender,
            reciever,
            add_peer_text: String::new(),
            frame: egui::Frame {
                inner_margin: Margin {
                    left: 10f32,
                    right: 10f32,
                    top: 10f32,
                    bottom: 10f32,
                },
                fill: Color32::from_rgb(250, 250, 250),
                ..Default::default()
            },
        }
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Ok(Some(PacketFromBackend::MessageRecieved((peer, message)))) =
            self.reciever.try_next()
        {
            if !self.chats.iter().any(|x| x.chat_peer_id == peer) {
                self.chats.push(Chat {
                    chat_name: String::from("new"),
                    chat_peer_id: format!("{}", peer),
                    messages: Vec::new(),
                });
            }
            self.chats
                .iter_mut()
                .find(|x| x.chat_peer_id == peer)
                .unwrap()
                .messages
                .push(Message {
                    sender: peer,
                    text: message,
                });
        }
        egui::TopBottomPanel::top("my_panel")
            .frame(self.frame)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let style: egui::Style = (*ui.ctx().style()).clone();
                    let visual = if style.visuals.dark_mode {
                        if ui
                            .add(Button::new("☀").frame(false))
                            .on_hover_text("Switch to light mode")
                            .clicked()
                        {
                            self.frame = self.frame.fill(Color32::from_rgb(250, 250, 250));
                            egui::style::Visuals::light()
                        } else {
                            egui::style::Visuals::dark()
                        }
                    } else {
                        if ui
                            .add(Button::new("🌙").frame(false))
                            .on_hover_text("Switch to dark mode")
                            .clicked()
                        {
                            self.frame = self.frame.fill(Color32::from_rgb(25, 25, 25));
                            egui::style::Visuals::dark()
                        } else {
                            egui::style::Visuals::light()
                        }
                    };
                    ui.ctx().set_visuals(visual);
                    ui.heading("Gossip");
                });
                ui.separator();
            });
        egui::SidePanel::left("left panel")
            .resizable(false)
            .frame(self.frame)
            .show(ctx, |ui| {
                ui.group(|ui| {
                    if self.chats.is_empty() {
                        ui.label("You have no decentralised chats, you lonely loser you. Add one below. ");
                    } else {
                        ui.label("Your decentralised chats: ");
                    }
                    ui.add_space(10f32);
                    for (i, chat) in self.chats.iter().enumerate() {
                        if ui
                            .add_sized(
                                eframe::emath::Vec2 {
                                    x: ui.available_width(),
                                    y: 0f32,
                                },
                                Button::new(RichText::new(
                                    String::from("🌝  ") + chat.chat_name.as_str(),
                                )),
                            )
                            .clicked()
                        {
                            self.chat_index = i;
                        };
                    }
                    ui.add_space(10f32);
                    ui.label("Add peer: ");
                    ui.horizontal(|ui| {
                        ui.text_edit_singleline(&mut self.add_peer_text);

                        if ui.button(String::from("+")).clicked() {
                            self.sender
                                .try_send(PacketFromFrontend::AddPeer(self.add_peer_text.clone()))
                                .unwrap();
                            self.chats.push(Chat { chat_name: String::from("new"), chat_peer_id: self.add_peer_text.clone().split('/').last().unwrap().to_string(), messages: Vec::new() });
                            self.add_peer_text.clear();
                        };
                    })
                })
            });
        egui::CentralPanel::default()
            .frame(self.frame)
            .show(ctx, |ui| {
                ui.group(|ui| {
                    ui.vertical(|ui| {
                        if !self.chats.is_empty() {
                            for message in &self.chats[self.chat_index].messages {
                                ui.group(|ui| {
                                    ui.label(RichText::new(&message.sender).heading());
                                    ui.label(&message.text);
                                });
                            }
                        }
                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.add_enabled_ui(!self.chats.is_empty(), |ui| {
                                if ui.button("Send").clicked() {
                                    self.sender
                                        .try_send(PacketFromFrontend::SendMessage((
                                            (&self.chats)[self.chat_index].chat_name.clone(),
                                            self.draft_text.clone(),
                                        )))
                                        .unwrap();
                                    self.chats[self.chat_index].messages.push(Message {
                                        sender: String::from("me"),
                                        text: self.draft_text.clone(),
                                    });
                                    //.unwrap_or(println!("Error sending!"));
                                    self.draft_text.clear();
                                }
                            });
                            TextEdit::singleline(&mut self.draft_text)
                                .desired_width(f32::INFINITY)
                                .show(ui);
                        })
                    })
                })
            });
    }
}

//fn main() {
//   task::block_on(start());
//}

async fn start(
    reciever: &mut mpsc::Receiver<PacketFromFrontend>,
    sender: &mut mpsc::Sender<PacketFromBackend>,
) {
    // Create a random PeerId
    let local_key = identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_key.public());
    println!("Local peer id: {:?}", local_peer_id);

    // Set up an encrypted DNS-enabled TCP Transport over the Mplex and Yamux protocols
    let transport = libp2p::development_transport(local_key.clone())
        .await
        .unwrap();

    // Create a Gossipsub topic
    let topic = Topic::new("new");

    // Create a Swarm to manage peers and events
    let mut swarm = {
        // To content-address message, we can take the hash of message and use it as an ID.
        let message_id_fn = |message: &GossipsubMessage| {
            let mut s = DefaultHasher::new();
            message.data.hash(&mut s);
            MessageId::from(s.finish().to_string())
        };

        // Set a custom gossipsub
        let gossipsub_config = gossipsub::GossipsubConfigBuilder::default()
            .heartbeat_interval(Duration::from_secs(10)) // This is set to aid debugging by not cluttering the log space
            .validation_mode(ValidationMode::Strict) // This sets the kind of message validation. The default is Strict (enforce message signing)
            .message_id_fn(message_id_fn) // content-address messages. No two messages of the
            // same content will be propagated.
            .build()
            .expect("Valid config");
        // build a gossipsub network behaviour
        let mut gossipsub: gossipsub::Gossipsub =
            gossipsub::Gossipsub::new(MessageAuthenticity::Signed(local_key), gossipsub_config)
                .expect("Correct configuration");

        // subscribes to our topic
        gossipsub.subscribe(&topic).unwrap();

        // build the swarm
        libp2p::Swarm::new(transport, gossipsub, local_peer_id)
    };

    // Listen on all interfaces and whatever port the OS assigns
    swarm
        .listen_on("/ip4/0.0.0.0/tcp/0".parse().unwrap())
        .unwrap();

    // Kick it off
    loop {
        select! {
                    packet = reciever.select_next_some() =>
                {
                match packet {
                PacketFromFrontend::SendMessage((_, message)) => {
                    swarm
                        .behaviour_mut()
                        .publish(topic.clone(), message).unwrap();
                    }
                    PacketFromFrontend::AddPeer(peer) => {

                    let address: libp2p::Multiaddr = peer.parse().expect("User to provide valid address.");
        match swarm.dial(address.clone()) {
            Ok(_) => println!("Dialed {:?}", address),
            Err(e) => println!("Dial {:?} failed: {:?}", address, e),
        };
                    }
            }
                }

                    event = swarm.select_next_some() => match event {
        SwarmEvent::Behaviour(GossipsubEvent::Message {
                            propagation_source: peer_id,
                            message_id: id,
                            message,
                        }) => {
                        println!(
                            "Got message: {} with id: {} from peer: {:?}",
                            String::from_utf8_lossy(&message.data),
                            id,
                            peer_id
                        );
                       sender.try_send(PacketFromBackend::MessageRecieved((format!("{}",peer_id), String::from_utf8_lossy(&message.data).to_string()))).unwrap();
                    }
                        SwarmEvent::NewListenAddr { address, .. } => {
                            println!("Listening on {}/p2p/{}", address, local_peer_id);
                        }
                        _ => {}
                    }
                }
    }
}
