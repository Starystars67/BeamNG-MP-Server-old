use std::net::{TcpListener, TcpStream, UdpSocket, SocketAddr};
use std::thread;
use std::io::{Write, BufReader, BufRead, BufWriter, Read};
use std::sync::{Arc, RwLock};
use std::collections::HashMap;
use uuid::Uuid;
use serde::Serialize;
use std::fs::File;
use std::path::Path;
use serde_json::Value;

const VERSION: &str = "0.0.4";

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
            socket.1.flush().expect("Error broadcasting");
        }
        Ok(())
    }

    pub fn broadcast_to_everyone_else(&mut self, msg: String, except: &Player) -> Result<(), &str> {
        for socket in &mut self.map {
            if !Player::eq(socket.0, except) {
                socket.1.write(msg.as_bytes()).expect("Error broadcasting");
                socket.1.flush().expect("Error broadcasting");
            }
        }
        Ok(())
    }

    pub fn send_private(&mut self, msg: String, to: &Player) -> Result<(), &str> {
        let stream = &mut self.map.get_mut(to).expect("No such player found in player list");
        stream.write(msg.as_bytes()).expect("Error writing to stream");
        stream.flush().expect("Error writing to stream");
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
            res.push(player.1.get_ref().local_addr().expect("Error reading socket address (why?)"));
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

    let (env, map, tcp_port, udp_port) = match read_server_config(){
        Ok(tuple) => { (tuple.0, tuple.1, tuple.2, tuple.3) }
        Err(msg) => { println!("{}", msg); return; }
    };

    let connections = Arc::new(RwLock::new(Connections::new()));

    match TcpListener::bind(format!("0.0.0.0:{}", tcp_port)) {
        Ok(listener) => {
            println!("\n\tTCP listening on {}", format!("0.0.0.0:{}", tcp_port));
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
                                let env_cl = env.clone();
                                thread::spawn(move || {
                                    handle(cons, stream, map_cl, env_cl);
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
            println!("\n\n\tCould not bind TCP to {}", tcp_port);
            return;
        }
    };

    match UdpSocket::bind(format!("0.0.0.0:{}", udp_port)) {
        Ok(udp) => {
            println!("\tUDP listening on {}", format!("0.0.0.0:{}", udp_port));
            udp_loop(udp, connections);
        }
        Err(_) => {
            println!("\tCould not bind UDP to {}", udp_port);
        }
    };
}

fn read_server_config<'a>() -> Result<(Arc<RwLock<String>>, Arc<RwLock<String>>, u16, u16), &'a str> {
    let mut cfg = match File::open(Path::new("cfg/server.json")) {
        Ok(file) => { file } Err(_) => {return Err("cfg/server.json was not found. Aborting");}
    };
    let mut s = String::new();
    cfg.read_to_string(&mut s).unwrap();
    let json = match serde_json::from_str::<Value>(s.as_str()) {
        Ok(value) => { value } Err(_) => {return Err("Error parsing server.json");}
    };
    let env = json["env"].to_string();
    let map = {
        let map = json["map"].to_string();
        if map == "null" {String::from("")}
        else {map}
    };
    let tcp_port = match json["tcp_port"].as_u64(){
        Some(port) if port < 65535 => { port as u16 } _ => { return Err("TCP port not defined or is greater than 65535"); }
    };
    let udp_port = match json["udp_port"].as_u64(){
        Some(port) if port < 65535 => { port as u16 } _ => { return Err("UDP port not defined or is greater than 65535"); }
    };
    Ok((Arc::new(RwLock::new(env)), Arc::new(RwLock::new(map)), tcp_port, udp_port))
}

fn handle(connections: Arc<RwLock<Connections>>, stream: TcpStream, map: Arc<RwLock<String>>, env: Arc<RwLock<String>>) {
    let (mut reader, writer) = stream.try_clone().map(|clone| {(BufReader::new(stream), BufWriter::new(clone))}).unwrap();
    let id = Uuid::new_v4().to_string();

    let player = match handshake(writer, &mut reader, &connections, id, &map, &env) {
        Ok(player) => {
            println!("Handshake successful");
            player
        }
        Err(string) => {
            println!("An error occurred!\n{}\nDisconnected client", string);
            return;
        }
    };

    match main_loop(reader, connections, player, map, env) {
        Ok(()) => {
            println!("Client successfully served");
        }
        Err(msg) => {
            println!("Error occurred after handshake:\n{}\nDisconnected client", msg);
        }
    };
}

fn handshake<'a>(mut writer: BufWriter<TcpStream>, reader: &'a mut BufReader<TcpStream>, connections: &'a Arc<RwLock<Connections>>, id: String, map: &Arc<RwLock<String>>, env: &Arc<RwLock<String>>) -> Result<Player, &'a str> {
    let map = map.read().unwrap();
    writer.write(format!("HOLA{}\n", id).as_bytes()).expect("Failed writing during handshake (client disconnected?)");
    if *map == "" {
        writer.write(b"MAPS\n").expect("Failed writing during handshake (client disconnected?)");
    } else {
        writer.write(format!("MAPC{}\n", *map).as_bytes()).expect("Failed writing during handshake (client disconnected?)");
    }
    writer.write(format!("VCHK{}\n", VERSION).as_bytes()).expect("Failed writing during handshake (client disconnected?)");
    writer.flush().expect("Failed writing during handshake (client disconnected?)");

    let player = match get_player(reader, id) {
        Ok(player) => {
            player
        }
        Err(msg) => {
            return Err(msg);
        }
    };

    match sync_env(&mut writer, &env) {
        Ok(_) => {}
        Err(_) => {return Err("Error syncing environment");}
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
                    let addr = reader.get_mut().local_addr().expect("Failed to read address (why?)");
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

fn sync_env<'a>(writer: &mut BufWriter<TcpStream>, env: &'a Arc<RwLock<String>>) -> Result<(), &'a str> {
    let env = env.read().unwrap();
    match writer.write(format!("ENVT{}\n", *env).as_bytes()) {
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

fn main_loop<'a>(mut reader: BufReader<TcpStream>, connections: Arc<RwLock<Connections>>, mut player: Player, map: Arc<RwLock<String>>, env: Arc<RwLock<String>>) -> Result<(), &'a str> {
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
                    online = handle_client_msg(s, &connections, &mut player, &map, &env);
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

fn handle_client_msg(msg: String, connections: &Arc<RwLock<Connections>>, player: &mut Player, map: &Arc<RwLock<String>>, env: &Arc<RwLock<String>>) -> bool {
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
            if msg.contains("!admin_plox") {
                match connections.broadcast_to_everyone_else(format!("SMSGPlayer {} is now admin\n", player.nickname), player) {
                    Ok(_) => {} Err(_) => {}
                }
                match connections.send_private(String::from("ADMN\n"), player) {
                    Ok(_) => {} Err(_) => {}
                }
            }
            else {
                match connections.broadcast(format!("CHAT{}\n", msg)) {
                    Ok(_) => {println!("Broadcasting CHAT: {}", msg);}
                    Err(msg) => {println!("Error sending (CHAT) via TCP: {}", msg);}
                }
            }
        }
        "MAPS" => {
            let mut map = map.write().unwrap();
            *map = msg;
            println!("Set map to {}", *map);
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
        "SENV" => {
            let mut env = env.write().unwrap();
            *env = msg;
            match connections.broadcast_to_everyone_else(format!("ENVT{}\n", *env), player) {
                Ok(_) => {} Err(msg) => {println!("Error sending (ENVT) via TCP: {}", msg);}
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
            let local = udp.local_addr().expect("Failed to read socket address (why?)");
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