use crate::multiqueue::*;
use crate::packet::*;
use anyhow::*;
use libloading::os::windows::Symbol as WinSymbol;
use libloading::*;
use std::ffi::CString;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::atomic::*;
use std::sync::*;
use std::time::Duration;

pub const PACKET_SIZE: usize = 1600;

type ClientConnectType = unsafe extern "stdcall" fn(i32, i16, *const char, i32, i32, i16) -> i16;
type SendType = unsafe extern "stdcall" fn(i16, *const u8, i16, i16, i16) -> i16;
type ReadType = unsafe extern "stdcall" fn(i16, *const u8, i16, i16) -> i16;
type CommandType = unsafe extern "stdcall" fn(u16, i16, *const u8, u16) -> i16;
type _VERSION = unsafe extern "stdcall" fn(i16, *const u8, i16, i16) -> i16;
type GetErrorType = unsafe extern "stdcall" fn(i16, *const u8) -> i16;

fn log<F, T>(msg: &str, mut f: F) -> T
where
    F: FnMut() -> T,
{
    let start = std::time::Instant::now();
    println!("{} start", msg);
    let rtn = f();
    println!("{} done {:?}", msg, start.elapsed());
    rtn
}
pub struct Rp1210 {
    bus: MultiQueue<J1939Packet>,
    api: Arc<Mutex<API>>,
}
struct API {
    id: i16,

    _lib: Library,
    client_connect_fn: WinSymbol<ClientConnectType>,
    send_fn: WinSymbol<SendType>,
    read_fn: WinSymbol<ReadType>,
    send_command_fn: WinSymbol<CommandType>,
    get_error_fn: WinSymbol<GetErrorType>,
}

impl API {
    fn new(id: &str) -> Result<API> {
        Ok(unsafe {
            let lib = Library::new(id.to_string())?;
            let client_connect: Symbol<ClientConnectType> =
                (&lib).get(b"RP1210_ClientConnect\0").unwrap();
            let send: Symbol<SendType> = (&lib).get(b"RP1210_SendMessage\0").unwrap();
            let send_command: Symbol<CommandType> = (&lib).get(b"RP1210_SendCommand\0").unwrap();
            let read: Symbol<ReadType> = (&lib).get(b"RP1210_ReadMessage\0").unwrap();
            let get_error: Symbol<GetErrorType> = (&lib).get(b"RP1210_GetErrorMsg\0").unwrap();
            API {
                id: 0,
                client_connect_fn: client_connect.into_raw(),
                send_fn: send.into_raw(),
                read_fn: read.into_raw(),
                send_command_fn: send_command.into_raw(),
                get_error_fn: get_error.into_raw(),
                _lib: lib,
            }
        })
    }
    fn send_command(&self, cmd: u16, buf: Vec<u8>) -> Result<i16> {
        self.verify_return(unsafe {
            (self.send_command_fn)(cmd, self.id, buf.as_ptr(), buf.len() as u16)
        })
    }
    fn get_error(&self, code: i16) -> Result<String> {
        let mut buf: [u8; 1024] = [0; 1024];
        let size = unsafe { (self.get_error_fn)(code, buf.as_mut_ptr()) } as usize;
        Ok(String::from_utf8_lossy(&buf[0..size]).to_string())
    }
    fn verify_return(&self, v: i16) -> Result<i16> {
        if v < 0 {
            Err(anyhow!(self.get_error(-v)?))
        } else {
            Ok(v)
        }
    }
    fn client_connect(
        &mut self,
        dev_id: i16,
        connection_string: &str,
        address: u8,
        app_packetize: bool,
    ) -> Result<i16> {
        let c_to_print = CString::new(connection_string).expect("CString::new failed");
        let id = unsafe {
            (self.client_connect_fn)(
                0,
                dev_id,
                c_to_print.as_ptr() as *const char,
                0,
                0,
                if app_packetize { 1 } else { 0 },
            )
        };
        self.id = self.verify_return(id)?;
        if !app_packetize {
            self.send_command(
                /*CMD_PROTECT_J1939_ADDRESS*/ 19,
                vec![
                    address, 0, 0, 0xE0, 0xFF, 0, 0x81, 0, 0, /*CLAIM_BLOCK_UNTIL_DONE*/ 0,
                ],
            )?;
        }
        self.send_command(
            /*CMD_ECHO_TRANSMITTED_MESSAGES*/ 16,
            vec![/*ECHO_ON*/ 1],
        )?;
        self.send_command(/*CMD_SET_ALL_FILTERS_STATES_TO_PASS*/ 3, vec![])?;
        Ok(id)
    }
    fn send(&self, packet: &J1939Packet) -> Result<i16> {
        let buf = &packet.packet.data;
        self.verify_return(unsafe { (self.send_fn)(self.id, buf.as_ptr(), buf.len() as i16, 0, 0) })
    }
}

#[allow(dead_code)]
impl Rp1210 {
    pub fn new(id: &str, bus: MultiQueue<J1939Packet>) -> Result<Rp1210> {
        Ok(Rp1210 {
            bus: bus.clone(),
            api: Arc::new(Mutex::new(API::new(id)?)),
        })
    }
    /// load DLL, make connection and background thread to read all packets into queue
    pub fn run(&mut self, dev: i16, connection: &str, address: u8) -> Result<Box<dyn Fn() -> ()>> {
        let closer = Arc::new(AtomicBool::new(true));
        let id = self.client_connect(dev, connection, address).unwrap();
        let mut bus = self.bus.clone();
        let api = self.api.clone();
        let running = closer.clone();
        std::thread::spawn(move || {
            let mut buf: [u8; PACKET_SIZE] = [0; PACKET_SIZE];
            while running.load(Relaxed) {
                // lock API, then read as long as there is traffic, then release lock so that sends can be made.
                {
                    let api = api.lock().expect("Unable to access RP1210 API");
                    let read = *api.read_fn;
                    while running.load(Relaxed) {
                        let size = unsafe { read(id, buf.as_mut_ptr(), PACKET_SIZE as i16, 0) };
                        if size > 0 {
                            bus.push(J1939Packet::new_rp1210(&buf[0..size as usize]))
                        } else {
                            if size < 0 {
                                // read error
                                let code = -size;
                                let size =
                                    unsafe { (*api.get_error_fn)(code, buf.as_mut_ptr()) } as usize;
                                let msg = String::from_utf8_lossy(&buf[0..size]).to_string();
                                println!("RP1210 error: {}: {}", code, msg);
                            }
                            break;
                        }
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(2));
                //std::thread::yield_now();
            }
        });
        Ok(Box::new(move || closer.store(false, Relaxed)))
    }

    pub fn client_connect(
        &mut self,
        dev_id: i16,
        connection_string: &str,
        address: u8,
    ) -> Result<i16> {
        self.api
            .lock()
            .expect("Unable to access RP1210 API")
            .client_connect(dev_id, connection_string, address, true)
    }

    pub fn send(&self, packet: &J1939Packet) -> Result<J1939Packet> {
        let mut stream = self.bus.iter_for(Duration::from_secs(2));
        let send = self
            .api
            .lock()
            .expect("Unable to access RP1210 API")
            .send(packet);
        send.map(|_| stream.find(move |p| p.data() == packet.data()).unwrap())
    }
}
