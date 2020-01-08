// Server Settings!
var map = "";

const net = require('net');
const uuidv4 = require('uuid/v4');
const args = require('minimist')(process.argv.slice(2));
const chalk = require("chalk")
//console.log(args.port)
if (args.port) {
  var tcpport = args.port;
} else {
  var tcpport = 30813;
}
var udpport = tcpport + 1;
var wsport = tcpport + 2;
const host = '192.168.1.195';

//==========================================================
//              WebSocket Server
//==========================================================

const WebSocket = require('ws');

const wss = new WebSocket.Server({ port: wsport });

wss.on('connection', function connection(ws) {
  ws.on('message', function incoming(message) {
    console.log('[WS] received: %s', message);
    wss.clients.forEach(function each(client) {
      if (client.readyState === WebSocket.OPEN) {
        client.send(message);
      }
    });
  });

  ws.send('Welcome!');
});
console.log(chalk.cyan('[WS]')+'  Server listening on 0.0.0.0:' + wsport);

//==========================================================
//              TCP Server
//==========================================================

const TCPserver = net.createServer();
TCPserver.listen(tcpport, () => {
  console.log(chalk.green('[TCP]')+' Server listening on 0.0.0.0:' + tcpport);
});

let sockets = [];
let players = [];
let names = [];
let vehicles = [];

TCPserver.on('connection', function(sock) {
  console.log(chalk.green('[TCP]')+' CONNECTED: ' + sock.remoteAddress + ':' + sock.remotePort);
  sockets.push(sock);

  var player = {};
  player.remoteAddress = sock.remoteAddress;
  player.remotePort = sock.remotePort;
  player.nickname = "New User, Loading...";
  player.id = uuidv4();
  player.currentVehID = 0;

  players.push(player);
  sockets.forEach(function(socket, index, array) { // Send update to all clients
    socket.write('PLST'+JSON.stringify(players)+'\n');
  });

  sock.write('HOLA'+player.id+'\n');
  if (map == "") {
    sock.write("MAPS\n");
  } else {
    sock.write("MAPC"+map+'\n')
  }

  sock.on('data', function(data) {
    // Write the data back to all the connected, the client will receive it as data from the server
    var str = data.toString();
    data = str.trim(); //replace(/\r?\n|\r/g, "");
    var code = data.substring(0, 4);
    var message = data.substr(4);

    if (code != "PING") {
      console.log(code)
  	}
  	//if (data.length > 4) {
  	  //console.log(data.length)
  	//}

    switch (code) {
      case "PING":
        //console.log("Ping Received")
        sock.write('PONG\n');
        break;
      case "CHAT":
        sockets.forEach(function(socket, index, array) { // Send update to all clients
          socket.write(data+'\n');
        });
        break;
      case "MAPS":
        map = message;
        console.log("Setting map to: "+map);
        sock.write("MAPC"+map+'\n');
        break;
      case "USER":
        players.forEach(function(player, index, array) {
          if (player.remoteAddress == sock.remoteAddress && player.remotePort == sock.remotePort) {
            console.log("Player Found ("+player.id+"), setting nickname("+data.substr(4)+")");
            player.nickname = ""+data.substr(4)+"";
            sockets.forEach(function(socket, index, array) { // Send update to all clients
              socket.write('PLST'+JSON.stringify(players)+'\n');
            });
          }
        });
        break;
      case "QUIT":
      case "2001":
        let index = sockets.findIndex(function(o) {
          return o.remoteAddress === sock.remoteAddress && o.remotePort === sock.remotePort;
        })
        if (index !== -1) sockets.splice(index, 1);
        console.log('CLOSED: ' + sock.remoteAddress + ' ' + sock.remotePort);
        break;
      case "U-VI":
      case "U-VE":
      case "U-VN":
      case "U-VP":
      case "U-VL":
        //console.log(data)
        //players.forEach(function(player, index, array) {
        //if (player.remoteAddress != sock.remoteAddress) {
        //console.log(player.remoteAddress+' != '+sock.remoteAddress+' Is not the same so we should send?')
        //console.log("Got Update to send!")
        sockets.forEach(function(socket, index, array) { // Send update to all clients
          //console.log(socket.remotePort+' != '+sock.remotePort+' Is not the same so we should send?')
          if ((sock.remoteAddress != socket.remoteAddress && sock.remotePort != socket.remotePort) || (sock.remoteAddress == socket.remoteAddress && sock.remotePort != socket.remotePort)) {
            socket.write(data+'\n');
          }
        });
        //}
        //});
        break;
      case "U-VC":
        sockets.forEach(function(socket, index, array) { // Send update to all clients
          socket.write(data+'\n');
        });
        break;
      case "U-NV":
        console.log(message)
        var vid = uuidv4();

        break;
      case "C-VS": // Client has changed vehicle. lets update our records.
        console.log(message)
        players.forEach(function(player, index, array) {
          if (player.currentVehID != message && player.remoteAddress == sock.remoteAddress && player.remotePort == sock.remotePort) {
            console.log(chalk.green('[TCP]')+" Player Found ("+player.id+"), updating current vehile("+message+")");
            player.currentVehID = message;
          }
        });
        break;
      default:
        console.log(chalk.green('[TCP]')+' Unknown / unhandled data from:' + sock.remoteAddress);
        console.log(chalk.green('[TCP]')+' Data -> ' + data);
        sockets.forEach(function(socket, index, array) { // Send update to all clients
          //if ((sock.remoteAddress != socket.remoteAddress && sock.remotePort != socket.remotePort) || (sock.remoteAddress == socket.remoteAddress && sock.remotePort != socket.remotePort)) {
            socket.write(data+'\n');
          //}
        });
        break;
    }
    sockets.forEach(function(sock, index, array) {
      //sock.write(sock.remoteAddress + ':' + sock.remotePort + " said " + data + '\n');
    });
  });

  // Add a 'close' event handler to this instance of socket
  sock.on('close', function(data) {
    var index = players.findIndex(function(o) {
      return o.remoteAddress === sock.remoteAddress && o.remotePort === sock.remotePort;
    })
    if (index !== -1) sockets.splice(index, 1);
     index = sockets.findIndex(function(o) {
      return o.remoteAddress === sock.remoteAddress && o.remotePort === sock.remotePort;
    })
    if (index !== -1) sockets.splice(index, 1);
    console.log('CLOSED: ' + sock.remoteAddress + ' ' + sock.remotePort);
    sockets.forEach(function(socket, index, array) { // Send update to all clients
      socket.write('PLST'+JSON.stringify(players)+'\n');
    });
  });

  sock.on('error', (err) => {
    // handle errors here
    if (err.code == "ECONNRESET") {
      console.error(chalk.red("ERROR ")+"Connection Reset for player: ");
      players.forEach(function(player, index, array) {
        if (player.remoteAddress == sock.remoteAddress && player.remotePort == sock.remotePort) {
          console.error(+player.nickname+" ("+player.id+")");
          console.error(chalk.red("ERROR ")+"End Error.");
        }
      });
    } else {
      console.error("Sock Error");
      console.error(err);
      throw err;
    }
  });
});

TCPserver.on('error', (err) => {
  // handle errors here
  console.error("TCPserver Error");
  console.error(err);
  throw err;
});

//==========================================================
//              UDP Server
//==========================================================

var dgram = require('dgram');
var UDPserver = dgram.createSocket('udp4');

function UDPsend(message, info) {
  UDPserver.send(message,info.port,info.address,function(error){if(error){console.log("ERROR");console.log(error);client.close();};});
}

UDPserver.on('listening', function() {
  var address = UDPserver.address();
  console.log(chalk.rgb(123, 45, 67)('[UDP]')+' Server listening on ' + address.address + ':' + address.port);
});

var UDPMembers = [];

UDPserver.on('message',function(msg,info){
  //sending msg
  var str = msg.toString();
  data = str.trim(); //replace(/\r?\n|\r/g, "");
  var code = data.substring(0, 4);
  switch (code) {
    case "PING":
    UDPsend("PONG", info)
    break;
    default:
    console.log(chalk.rgb(123, 45, 67)('[UDP]')+' Data received from client : ' + msg.toString());
    console.log(chalk.rgb(123, 45, 67)('[UDP]')+' Received %d bytes from %s:%d\n',msg.length, info.address, info.port);
  }
});

UDPserver.bind(udpport);
