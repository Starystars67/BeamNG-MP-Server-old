use std::net::{TcpListener, TcpStream, UdpSocket, SocketAddr};
use std::thread;
use std::io::{Write, BufReader, BufRead, BufWriter};
use std::sync::{Arc, RwLock};
use std::collections::HashMap;
use uuid::Uuid;
use serde::Serialize;

const VERSION: &str = "0.0.3";

struct Connections {
    map: HashMap<Player, BufWriter<TcpStream>>
}

#[derive(PartialEq, Eq, Hash)]
#[derive(Serialize)]
struct Player {
    remote_address: String,
    remote_port: u16,
    nickname: String,
    id: String,
    #[serde(rename="currentVehID")]
    current_veh_id: String
}

impl Connections {
    pub fn new() -> Connections{
        Connections { map: HashMap::new()}
    }

    pub fn broadcast(&mut self, msg: String) -> Result<(), &str> {
        for socket in &mut self.map {
            socket.1.write(msg.as_bytes()).expect("Error broadcasting");
            socket.1.flush().unwrap();
        }
        Ok(())
    }

    pub fn broadcast_to_everyone_else(&mut self, msg: String, except: &Player) -> Result<(), &str> {
        for socket in &mut self.map {
            if !Player::eq(socket.0, except) {
                socket.1.write(msg.as_bytes()).expect("Error broadcasting");
                socket.1.flush().unwrap();
            }
        }
        Ok(())
    }

    pub fn send_private(&mut self, msg: String, to: &Player) -> Result<(), &str> {
        let stream = &mut self.map.get_mut(to).expect("No such player found in player list");
        stream.write(msg.as_bytes()).expect("Error writing to stream");
        stream.flush().unwrap();
        Ok(())
    }

    pub fn add_player(&mut self, player: Player, writer: BufWriter<TcpStream>) {
        self.map.insert(player, writer);
    }

    pub fn remove_player(&mut self, player: &Player) {
        self.map.remove(player);
    }

    pub fn get_list_of_players(&self) -> Vec<&Player> {
        let mut res = vec![];
        for pair in &self.map {
            res.push(pair.0);
        }
        res
    }

    pub fn get_addresses(&self) -> Vec<SocketAddr> {
        let mut res: Vec<SocketAddr> = vec![];
        for player in &self.map {
            res.push(player.1.get_ref().local_addr().unwrap());
        }
        res
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }
}

impl Player {
    pub fn new(remote_address: String,
               remote_port: u16,
               nickname: String,
               id: String,
               current_veh_id: String) -> Player {
        Player {
            remote_address,
            remote_port,
            nickname,
            id,
            current_veh_id
        }
    }

    pub fn copy(other: &Player) -> Player {
        Player {
            remote_address: other.remote_address.clone(),
            remote_port: other.remote_port,
            nickname: other.nickname.clone(),
            id: other.id.clone(),
            current_veh_id: other.current_veh_id.clone()
        }
    }

    pub fn eq(this: &Player, other: &Player) -> bool {
        this.id==other.id
    }
}

fn main() {

    let env = "{\"azimuthOverride\" = 0,\"nightScale\" = 1.5, \"time\" = 0, \"dayLength\" = 120, \"dayScale\" = 1, \"play\" = false}/0/-9.81/0";
    let map = Arc::new(RwLock::new(String::new()));
    let connections = Arc::new(RwLock::new(Connections::new()));

    print!("Enter port number to open TCP on (leave empty for 30813): ");
    std::io::stdout().flush().unwrap();
    let tcp_port = {
        let mut tcp_port = String::new();
        if std::io::stdin().read_line(&mut tcp_port).unwrap()<=2 {
            30813u16
        } else {
            match tcp_port.trim().parse::<u16>() {
                Ok(val) => {
                    val
                }
                Err(_) => {
                    println!("Could not convert to u16, setting to 30813");
                    30813u16
                }
            }
        }
    };

    match TcpListener::bind(format!("0.0.0.0:{}", tcp_port)) {
        Ok(listener) => {
            println!("\nTCP listening on {}", format!("0.0.0.0:{}", tcp_port));
            let tcp_cons = connections.clone();
            thread::spawn(move || {
                for stream in listener.incoming() {
                    match stream {
                        Ok(stream) => {
                            println!("Got connection!");
                            let cons = tcp_cons.clone();
                            let size_lock = tcp_cons.read().unwrap();
                            if size_lock.len() < 8 {
                                let map_cl = map.clone();
                                thread::spawn(move || {
                                    handle(cons, stream, map_cl, env);
                                });
                            } else {
                                println!("Denied: Server full (max 8 players)");
                            }
                        }
                        Err(_) => {
                            println!("Something went wrong while accepting incoming request!");
                        }
                    }
                }
            });
        }
        Err(_) => {
            //println!("Could not open server on {}:{}", tcp_ip, tcp_port); THIS IS DEBUG
        }
    };

    match UdpSocket::bind(format!("0.0.0.0:{}", tcp_port+1)) {
        Ok(udp) => {
            println!("UDP listening on {}", format!("0.0.0.0:{}", tcp_port+1));
            udp_loop(udp, connections.clone());
        }
        Err(_) => {
            println!("Could not bind UDP to {}", tcp_port+1);
        }
    }
}

fn handle(connections: Arc<RwLock<Connections>>, stream: TcpStream, map: Arc<RwLock<String>>, env: &str) {
    let (mut reader, writer) = stream.try_clone().map(|clone| {(BufReader::new(stream), BufWriter::new(clone))}).unwrap();
    let id = Uuid::new_v4().to_string();

    let player = match handshake(writer, &mut reader, &connections, id, &map, env) {
        Ok(player) => {
            println!("Handshake successful");
            player
        }
        Err(string) => {
            println!("An error occurred!\n{}", string);
            return;
        }
    };

    match main_loop(reader, connections, player, map) {
        Ok(()) => {
            println!("Client successfully served");
        }
        Err(msg) => {
            println!("Error occurred after handshake:\n{}", msg);
        }
    };
}

fn handshake<'a>(mut writer: BufWriter<TcpStream>, reader: &'a mut BufReader<TcpStream>, connections: &'a Arc<RwLock<Connections>>, id: String, map: &Arc<RwLock<String>>, env: &str) -> Result<Player, &'a str> {
    writer.write(format!("HOLA{}\n", id).as_bytes()).unwrap();
    if *map.read().unwrap() == "" {
        writer.write(b"MAPS\n").unwrap();
    } else {
        writer.write(format!("MAPC{}\n", *map.read().unwrap()).as_bytes()).unwrap();
    }
    writer.write(format!("VCHK{}\n", VERSION).as_bytes()).unwrap();
    writer.flush().unwrap();

    let player = match get_player(reader, id) {
        Ok(player) => {
            player
        }
        Err(msg) => {
            return Err(msg);
        }
    };

    match sync_env(&mut writer, env) {
        Ok(_) => {}
        Err(msg) => {return Err("Error syncing environment");}
    }

    match update_players_list_and_send(&player, connections, Option::Some(writer), true) {
        Ok(_) => {
            Ok(player)
        }
        Err(msg) => {
            println!("{}", msg);
            Err("Could not update players list for some reason. Msg why is above")
        }
    }
}

fn get_player(reader: & mut BufReader<TcpStream>, id: String) -> Result<Player, &str> {
    let mut count = 0u8;
    while count < 10 {
        let mut s = String::new();
        match reader.read_line(&mut s) {
            Ok(size) if size > 4 => {
                if &s[..4]=="USER" {
                    let addr = reader.get_mut().local_addr().unwrap();
                    return Ok(Player::new(addr.ip().to_string(),
                                          addr.port(),
                                          s[4..].trim().to_string(),
                                          id,
                                          String::from("0")));
                }
                count +=1;
            }
            _ => {return Err("Client disconnected during handshake");}
        }
    }
    Err("Client did not give information about themselves (\"USER\" code was not received)")
}

fn sync_env<'a>(writer: &'a mut BufWriter<TcpStream>, env: &str) -> Result<(), &'a str> {
    match writer.write(format!("ENVT{}\n", env).as_bytes()) {
        Ok(_) => {}
        Err(_) => {return Err("Error sending environment");}
    }
    Ok(())
}

fn update_players_list_and_send<'a>(player: &Player, connections: &'a Arc<RwLock<Connections>>, writer: Option<BufWriter<TcpStream>>, op: bool) -> Result<usize, &'a str> {
    let mut connections = connections.write().unwrap();
    if op {connections.add_player(Player::copy(player), writer.unwrap());}
    else {connections.remove_player(player);}
    let list = connections.get_list_of_players();
    let list = serde_json::to_string(&list).expect("Error parsing json list");
    match connections.broadcast(format!("PLST{}\n", list)) {
        Ok(_) => {}
        Err(msg) => {println!("{}", msg); return Err("Error sending PLST");}
    };
    Ok(connections.len())
}

fn main_loop<'a>(mut reader: BufReader<TcpStream>, connections: Arc<RwLock<Connections>>, mut player: Player, map: Arc<RwLock<String>>) -> Result<(), &'a str> {
    let mut online = false;
    let mut count = 0u64;
    let mut acc = 0u64;
    loop {
        let mut s = String::new();
        let check = match reader.read_line(&mut s) {
            Ok(size) => {
                if size > 100 {
                    count += 1;
                    acc += size as u64;
                    if count == 100 {println!("TCP: average (over 100 reads longer than 100 bytes) is {} bytes", acc/count); count = 0;}
                }
                if size > 3 {
                    online = handle_client_msg(s, &connections, &mut player, &map);
                } else {
                    on_close(&connections, &mut player, &map);
                    online = false;
                }
                Ok(())
            }
            Err(_) => {
                on_close(&connections, &mut player, &map);
                Err("Error in main loop. Client unexpectedly disconnected.")
            }
        };
        match check {
            Ok(()) => {
                if online {
                    continue;
                }
                else {break;}
            }
            Err(msg) => {
                return Err(msg);
            }
        }
    }
    Ok(())
}

fn handle_client_msg(msg: String, connections: &Arc<RwLock<Connections>>, player: &mut Player, map: &Arc<RwLock<String>>) -> bool {
    let msg = msg.trim();
    let code = &msg[..4];
    let msg = msg[4..].to_string();

    if code == "QUIT" || code == "2001" {on_close(connections, player, map); return false;}

    let mut connections = connections.write().unwrap();
    match code {
        "PING" => {
            match connections.send_private(String::from("PONG\n"), player) {
                Ok(()) => {}
                Err(msg) => { println!("Error sending (PONG) via TCP: {}", msg); }
            }
        }
        "CHAT" => {
            match connections.broadcast(format!("CHAT{}\n", msg)) {
                Ok(_) => {println!("Broadcasting CHAT: {}", msg);}
                Err(msg) => {println!("Error sending (CHAT) via TCP: {}", msg);}
            }
        }
        "MAPS" => {
            let mut map = map.write().unwrap();
            *map = msg;
            match connections.send_private(format!("MAPC{}\n", *map), player) {
                Ok(()) => {}
                Err(msg) => {println!("Error sending (MAPC) via TCP: {}", msg);}
            }
        }
        "U-VI" | "U-VE" | "U-VN" | "U-VP" | "U-VL" | "U-VR" => {
            match connections.broadcast_to_everyone_else(format!("{}\n", msg), player) {
                Ok(_) => {}
                Err(msg) => {println!("Error sending (U-V[I/E/N/P/L/R]) via TCP: {}", msg);}
            }
        }
        "U-VC" => {
            match connections.broadcast(format!("{}\n", msg)) {
                Ok(_) => {}
                Err(msg) => {println!("Error sending (U-VC) via TCP: {}", msg);}
            }
        }
        "U-NV" => {
            println!("U-NV:\n{}", msg);
            //TODO new id???
        }
        "C-VS" => {
            println!("C-VS:\n{}", msg);
            if player.current_veh_id != msg {
                player.current_veh_id = msg;
            }
        }
        _ => {
            println!("Unknown request from {}:{} (nickname: {}):\n{}", player.remote_address,
                                                        player.remote_port,
                                                        player.nickname,
                                                        msg);
        }
    }
    true
}

fn on_close(connections: &Arc<RwLock<Connections>>, player: &mut Player, map: &Arc<RwLock<String>>) {
    println!("Player {} disconnected", player.nickname);
    match update_players_list_and_send(player, connections, Option::None, false) {
        Ok(remaining) => {
            if remaining==0 {
                let mut map = map.write().unwrap();
                *map = String::from("");
            }
        }
        Err(msg) => {
            println!("Error closing: {}", msg);
        }
    }
}

fn udp_loop(mut udp: UdpSocket, connections: Arc<RwLock<Connections>>) {
    let mut count = 0u64;
    let mut acc = 0u64;
    loop {
        let mut s = [0u8; 2048];
        match udp.recv_from(&mut s) {
            Ok(tuple) if tuple.0 > 3 => {
                let s = match std::str::from_utf8(&s) {
                    Ok(string) => {
                        string
                    }
                    Err(_) => {
                        println!("Non-UTF-8 was received.");
                        continue;
                    }
                };
                if tuple.0 > 100 {
                    count += 1;
                    acc += tuple.0 as u64;
                    if count == 100 {println!("UDP: average (over 100 reads longer than 100 bytes) is {} bytes", acc/count); count = 0;}
                    println!("UDP: {} bytes", tuple.0);
                }
                handle_udp_request(s, tuple.1, &mut udp, &connections);
            }
            _ => {
                println!("Error receiving from UDP");
            }
        };
    }
}

fn handle_udp_request(string: &str, addr: SocketAddr, udp: &mut UdpSocket, connections: &Arc<RwLock<Connections>>) {
    let code = &string[..4];
    let msg = &string[4..];
    match code {
        "PING" => {
            match udp.send_to(b"PONG\n", addr) {
                Ok(_) => {}
                Err(_) => { println!("Error sending (PONG) via UDP"); }
            }
        }
        "U-VI" | "U-VE" | "U-VN" | "U-VP" | "U-VL" | "U-VR" => {
            let local = udp.local_addr().unwrap();
            let addr = connections.read().unwrap().get_addresses();
            for unit in addr {
                if unit != local {
                    match udp.send_to(msg.as_bytes(), unit) {
                        Ok(_) => {}
                        Err(_) => { println!("Error sending (U-V[I/E/N/P/L/R]) via UDP"); }
                    }
                }
            }
        }
        "U-VC" | _ => {
            let addr = connections.read().unwrap().get_addresses();
            for unit in addr {
                match udp.send_to(msg.as_bytes(), unit) {
                    Ok(_) => {}
                    Err(_) => { println!("Error sending (U-VC or unhandled) via UDP"); }
                }
            }
        }
    }
}