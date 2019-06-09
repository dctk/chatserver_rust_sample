///
/// 簡易チャットサーバー
/// プロトコル：
///   リトルエンディアン
/// 　Header(1byte) : 0xFF
///   Length(4byte) : (i32)
///   Message(byte) : (text)
/// 

extern crate mio;
extern crate byteorder;

use std::collections::{HashMap};
use std::io::prelude::*;
use std::io::{Cursor};
use mio::{Poll,Events,Token,PollOpt,Ready};
use mio::tcp::{TcpListener,TcpStream,Shutdown};
use std::net::{SocketAddr,IpAddr,Ipv4Addr};
use byteorder::{ReadBytesExt, LittleEndian};

struct Client{
    addr : SocketAddr,
    stream: TcpStream,
    receive_buff: Vec<u8>
}

impl Client{
    fn new(stream: TcpStream,addr: SocketAddr)->Self{
        Client{ addr,stream,receive_buff:Vec::new() }
    }
}

struct ClientManager{
    clients : HashMap<Token,Client>,
    free_tokens:Vec<Token>,
    next_token : Token
}

impl ClientManager{
    fn new(init_token : Token)->Self{
        ClientManager{
            clients: HashMap::new(),
            free_tokens : Vec::new(),
            next_token : init_token,
        }
    }

    fn next_token(&mut self)->Token{
        if let Some(token) = self.free_tokens.pop(){
            return token;
        }
        let next_token = self.next_token;
        self.next_token = Token(self.next_token.0+1);
        return next_token;
    }

    fn add_client(&mut self,token :Token,client : Client){
        self.clients.insert(token, client);
    }

    fn remove_client(&mut self,token:Token){
        let r = self.clients.remove(&token);
        if r.is_some(){
            self.free_tokens.push(token);
        }
    }

    fn get_mut(&mut self,token: &Token)->Option<&mut Client>{
        self.clients.get_mut(token)
    }

    fn transfer_all(&mut self,buff:&[u8]){
        for client in self.clients.values_mut(){
            match client.stream.write(buff){
                Ok(_)=>{},
                Err(_)=>{ }
            }
        }
    }
}

fn main(){
   let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),1024);
   server(&addr);
}

fn server(addr: &SocketAddr){
    println!("start server : {}",addr);

    // preparation
    const SERVER_TOKEN: Token = Token(0);
    let mut client_mng = ClientManager::new(Token(1));

    // bind
    let listener = TcpListener::bind(&addr).unwrap();

    // start listener
    let poll = Poll::new().unwrap(); 
    poll.register(
        &listener, 
        SERVER_TOKEN, 
        Ready::readable(), 
        PollOpt::edge()).unwrap();
    let mut events = Events::with_capacity(1024);

    // server loop
    let mut messages:Vec<u8> = Vec::new();
    loop{
        poll.poll(&mut events, None).unwrap();
        for event in events.iter(){
            match event.token(){
                // accept
                SERVER_TOKEN=>{
                    let (stream,client_addr)=listener.accept().unwrap();
                    println!("connection client : {}",client_addr);

                    let token = client_mng.next_token();                    
                    poll.register(&stream, token, Ready::readable(), PollOpt::edge()).unwrap();
                    client_mng.add_client(token, Client::new(stream,client_addr));
                }
                // receive from client
                token=>{
                    let mut closed = false;
                    if let Some(client) = client_mng.get_mut(&token){
                        let mut buff = [0;1024];
                        match client.stream.read(&mut buff){
                            Ok(n)=>{
                                if n == 0 {
                                    println!("disconnection client : {}",client.addr);
                                    poll.deregister(&client.stream).unwrap();
                                    client.stream.shutdown(Shutdown::Both).unwrap();    
                                    closed = true;                   
                                }
                                else{
                                    client.receive_buff.extend_from_slice(&buff[0..n]);

                                    // analyze data
                                    let mut message_len: usize = 0;
                                    loop{
                                        if let Some(n) = client.receive_buff.iter().position(|&r|r==0xFF){
                                            if n > 0{
                                                client.receive_buff.resize(client.receive_buff.len()-n,0);
                                            }
                                        }
                                        else{
                                            client.receive_buff.clear();
                                            break;
                                        } 

                                        if client.receive_buff.len() >= 5{
                                            let mut rdr = Cursor::new(vec![
                                                client.receive_buff[1],
                                                client.receive_buff[2],
                                                client.receive_buff[3],
                                                client.receive_buff[4],
                                            ]);
                                            message_len = rdr.read_i32::<LittleEndian>().unwrap() as usize;
                                        }
                                        else{
                                            break;
                                        }
                                        
                                        if message_len+5 <= client.receive_buff.len(){
                                            messages.extend_from_slice(&client.receive_buff[..message_len+5]); 
                                            client.receive_buff.resize(client.receive_buff.len()-(message_len+5), 0);                          
                                        }     
                                        else{
                                            break;
                                        }
                                    }
                                }
                            },
                            Err(_)=>{}
                        }
                        
                    }
                    if closed{
                        client_mng.remove_client(token);
                    }
                }
            }
        }

        // クライアント全員にメッセージを配送する
        if messages.len()>0{
            let array = &messages[..];
            client_mng.transfer_all(&array);
            messages.clear();
        }        
    }
}
