use std::net::{IpAddr, UdpSocket};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Instant;
type Packet = Box<[u8]>;
struct Queue {
    rx: Receiver<Packet>,
    last_sent: Instant,
    socket: UdpSocket,
}

impl Queue {
    fn new_channel(address: IpAddr, port: u16) -> (Self, Sender<Packet>) {
        let (tx, rx) = channel();

        let socket = if address.is_ipv4() {
            UdpSocket::bind("0.0.0.0:0")
        } else {
            UdpSocket::bind("[::]:0")
        }
        .unwrap();

        socket.connect((address, port)).unwrap();

        (
            Self {
                rx,
                last_sent: Instant::now(),
                socket,
            },
            tx,
        )
    }

    fn last_sent(&self) -> Instant {
        self.last_sent
    }

    fn send(&mut self) {
        self.last_sent = Instant::now();
        if let Ok(packet) = self.rx.try_recv() {
            self.socket.send(&packet).unwrap();
        }
    }
}

use indexmap::IndexMap;
use std::collections::HashMap;
use std::sync::mpsc::{sync_channel, SyncSender};
use std::sync::{Arc, Mutex};
use std::time::Duration;
struct Manager {
    senders: Arc<Mutex<HashMap<i64, Sender<Packet>>>>,
    queues: Arc<Mutex<IndexMap<i64, Queue>>>,
    stop_tx: SyncSender<()>,
    stop_rx: Receiver<()>,
    capacity: usize,
    interval: Duration,
}

impl Manager {
    fn new(capacity: usize, interval: Duration) -> Self {
        let (stop_tx, stop_rx) = sync_channel(0);
        Self {
            senders: Default::default(),
            queues: Default::default(),
            stop_tx,
            stop_rx,
            capacity,
            interval,
        }
    }

    const TICK: Duration = Duration::from_millis(1);

    fn process(&self) {
        use std::sync::mpsc::TryRecvError;

        while let Err(TryRecvError::Empty) = self.stop_rx.try_recv() {
            if let Some((key, mut current_queue)) = self
                .queues
                .lock()
                .ok()
                .and_then(|mut q| q.shift_remove_index(0))
            {
                let stamp = Instant::now();
                if stamp.duration_since(current_queue.last_sent()) >= self.interval {
                    current_queue.send();
                }

                self.queues.lock().unwrap().insert(key, current_queue);
            }

            std::thread::sleep(Self::TICK);
        }
    }

    fn delete_queue(&self, key: i64) -> bool {
        if let Ok(mut senders) = self.senders.lock() {
            if let Ok(mut queues) = self.queues.lock() {
                senders.remove(&key);
                queues.shift_remove(&key);
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    fn queue_packet(&self, key: i64, address: String, port: u16, data: Vec<u8>) -> bool {
        let packet: Packet = data.into_boxed_slice();
        if let Ok(mut senders) = self.senders.lock() {
            if let Some(sender) = senders.get(&key) {
                sender.send(packet).is_ok()
            } else {
                let address = match dns_lookup::lookup_host(&address) {
                    Ok(vec) if !vec.is_empty() => vec[0],
                    _ => return false,
                };
                let (queue, sender) = Queue::new_channel(address, port);
                if let Ok(mut queues) = self.queues.lock() {
                    queues.insert(key, queue);
                }
                let ok = sender.send(packet).is_ok();
                senders.insert(key, sender);
                ok
            }
        } else {
            false
        }
    }

    fn remaining_capacity(&self) -> usize {
        self.capacity
    }

    fn stop(&self) {
        self.stop_tx.send(()).ok();
    }
}

lazy_static::lazy_static! {
    static ref MANAGER_STORAGE: Arc<Mutex<HashMap<usize, Manager>>> = Default::default();
}

use jni::objects::{JByteBuffer, JObject, JString};
use jni::sys::{jboolean, jint, jlong, jobject, jstring};
use jni::JNIEnv;
use std::ffi::c_void;

#[no_mangle]
pub extern "system" fn Java_com_sedmelluq_discord_lavaplayer_udpqueue_natives_UdpQueueManagerLibrary_create(
    _jni: JNIEnv,
    _me: JObject,
    queue_buffer_capacity: jint,
    packet_interval: jlong,
) -> jlong {
    let manager = Manager::new(
        queue_buffer_capacity as usize,
        Duration::from_nanos(packet_interval as u64),
    );
    let mut storage = MANAGER_STORAGE.lock().unwrap();
    let key = storage.len();
    storage.insert(key, manager);
    key as jlong
}

#[no_mangle]
pub extern "system" fn Java_com_sedmelluq_discord_lavaplayer_udpqueue_natives_UdpQueueManagerLibrary_destroy(
    _jni: JNIEnv,
    _me: JObject,
    instance: jlong,
) {
    if let Ok(mut storage) = MANAGER_STORAGE.lock() {
        if let Some(removed) = storage.remove(&(instance as usize)) {
            removed.stop();
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_com_sedmelluq_discord_lavaplayer_udpqueue_natives_UdpQueueManagerLibrary_getRemainingCapacity(
    _jni: JNIEnv,
    _me: JObject,
    instance: jlong,
    _key: jlong,
) -> jint {
    if let Ok(storage) = MANAGER_STORAGE.lock() {
        if let Some(manager) = storage.get(&(instance as usize)) {
            manager.remaining_capacity() as i32
        } else {
            0
        }
    } else {
        0
    }
}

#[no_mangle]
pub extern "system" fn Java_com_sedmelluq_discord_lavaplayer_udpqueue_natives_UdpQueueManagerLibrary_queuePacket(
    jni: JNIEnv,
    _me: JObject,
    instance: jlong,
    key: jlong,
    address_string: JString,
    port: jint,
    data_buffer: JByteBuffer,
    _data_length: jint,
) -> jboolean {
    let address = jni
        .get_string(address_string)
        .expect("Couldn't get java string!")
        .into();
    let bytes = jni
        .get_direct_buffer_address(data_buffer)
        .expect("Couldn't get java ByteBuffer!")
        .to_vec();

    if let Ok(storage) = MANAGER_STORAGE.lock() {
        if let Some(manager) = storage.get(&(instance as usize)) {
            if manager.queue_packet(key, address, port as u16, bytes) {
                jni::sys::JNI_TRUE
            } else {
                jni::sys::JNI_FALSE
            }
        } else {
            jni::sys::JNI_FALSE
        }
    } else {
        jni::sys::JNI_FALSE
    }
}

#[no_mangle]
pub extern "system" fn Java_com_sedmelluq_discord_lavaplayer_udpqueue_natives_UdpQueueManagerLibrary_queuePacketWithSocket(
    _jni: JNIEnv,
    _me: JObject,
    _instance: jlong,
    _key: jlong,
    _address_string: jstring,
    _port: jint,
    _data_buffer: jobject,
    _data_length: jint,
    _socket_handle: jlong,
) -> jboolean {
    // It does not work. Also, this method isn't used in jda-nas.
    /*let boxed_manager: Box<Manager> = unsafe { Box::from_raw(instance as *mut Manager) };

    let address: String = JString::from(address_string).into();
    let bytes = JByteBuffer::from(data_buffer);

    return if boxed_manager.queue_packet(
        key as u64,
        address,
        port,
        bytes.into(),
        data_length as usize,
        Some(unsafe { Box::from_raw(socket_handle as *mut UdpSocket) }.as_sock()),
    ) {
        jni::sys::JNI_TRUE
    } else {
        jni::sys::JNI_FALSE
    }*/
    unimplemented!("queuePacketWithSocket is not implemented!")
}

#[no_mangle]
pub extern "system" fn Java_com_sedmelluq_discord_lavaplayer_udpqueue_natives_UdpQueueManagerLibrary_deleteQueue(
    _jni: JNIEnv,
    _me: JObject,
    instance: jlong,
    key: jlong,
) -> jboolean {
    if let Ok(storage) = MANAGER_STORAGE.lock() {
        if let Some(manager) = storage.get(&(instance as usize)) {
            if manager.delete_queue(key) {
                jni::sys::JNI_TRUE
            } else {
                jni::sys::JNI_FALSE
            }
        } else {
            jni::sys::JNI_FALSE
        }
    } else {
        jni::sys::JNI_FALSE
    }
}

#[no_mangle]
pub extern "system" fn Java_com_sedmelluq_discord_lavaplayer_udpqueue_natives_UdpQueueManagerLibrary_process(
    _jni: JNIEnv,
    _me: JObject,
    instance: jlong,
) {
    if let Ok(storage) = MANAGER_STORAGE.lock() {
        if let Some(manager) = storage.get(&(instance as usize)) {
            manager.process();
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_com_sedmelluq_discord_lavaplayer_udpqueue_natives_UdpQueueManagerLibrary_processWithSocket(
    _jni: JNIEnv,
    _me: JObject,
    instance: jlong,
    _socket_v4: jlong,
    _socket_v6: jlong,
) {
    if let Ok(storage) = MANAGER_STORAGE.lock() {
        if let Some(manager) = storage.get(&(instance as usize)) {
            manager.process();
        }
    }
}

#[allow(dead_code)]
fn waiting_iterate_callback(
    _class_tag: jlong,
    _size: jlong,
    _tag_ptr: *mut jlong,
    _length: jint,
    _user_data: *mut c_void,
) -> jint {
    unimplemented!("waiting_iterate_callback is not implemented!")
}

#[no_mangle]
pub extern "system" fn Java_com_sedmelluq_discord_lavaplayer_udpqueue_natives_UdpQueueManagerLibrary_pauseDemo(
    _jni: JNIEnv,
    _me: JObject,
    _length: jint,
) {
    unimplemented!("pauseDemo is not implemented!")
}
