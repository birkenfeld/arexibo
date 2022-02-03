// Xibo player Rust implementation, (c) 2022 Georg Brandl.
// Licensed under the GNU AGPL, version 3 or later.

//! Receive, decrypt and handle incoming XMR messages from CMS.

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, FixedOffset, offset::Utc};
use crossbeam_channel::{Receiver, Sender, unbounded};
use rsa::RsaPrivateKey;
use serde::{Deserialize, Deserializer};
use serde_json::from_slice;
use crate::config::CmsSettings;

/// Possible messages to forward to the collect thread.
#[derive(Debug)]
pub enum Message {
    CollectNow,
    Screenshot,
}

pub struct Manager {
    private_key: RsaPrivateKey,
    sender: Sender<Message>,
    #[allow(unused)]  // need to hold onto the context
    context: zmq::Context,
    socket: zmq::Socket,
}

const HEARTBEAT: &[u8] = b"H";

impl Manager {
    pub fn new(settings: &CmsSettings, connect: &str,
               private_key: RsaPrivateKey) -> Result<(Self, Receiver<Message>)> {
        let channel = settings.xmr_channel();
        let context = zmq::Context::new();
        let socket = context.socket(zmq::SUB).context("creating XMR socket")?;
        socket.connect(connect).context("connecting XMR socket")?;
        socket.set_linger(0)?;
        socket.set_subscribe(channel.as_bytes())?;
        socket.set_subscribe(HEARTBEAT)?;
        let (sender, receiver) = unbounded();

        Ok((Self {
            private_key,
            sender,
            context,
            socket,
        }, receiver))
    }

    pub fn run(mut self) {
        loop {
            if let Err(e) = self.process_msg() {
                log::error!("handling XMR message: {:#}", e);
            }
        }
    }

    fn process_msg(&mut self) -> Result<()> {
        let channel = self.socket.recv_msg(0)?;
        assert!(channel.get_more());
        let key = self.socket.recv_msg(0)?;
        assert!(key.get_more());
        let content = self.socket.recv_msg(0)?;
        assert!(!content.get_more());
        if &*channel != HEARTBEAT {
            let json_msg = JsonMessage::new(&self.private_key, &key, &content)?;
            log::debug!("got XMR message: {:?}", json_msg);
            if let Some(msg) = json_msg.into_msg() {
                self.sender.send(msg).unwrap();
            }
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct JsonMessage {
    action: String,
    #[serde(rename = "createdDt")]
    #[serde(deserialize_with = "deserialize_datetime")]
    created: DateTime<FixedOffset>,
    #[serde(default)]
    ttl: i64,
}

impl JsonMessage {
    fn new(private_key: &RsaPrivateKey, key: &[u8], content: &[u8]) -> Result<Self> {
        let enc_key = base64::decode(key)?;
        let mut msg = base64::decode(content)?;
        let msg_key = decrypt_private_key(&enc_key, private_key)?;
        arc4::Arc4::with_key(&msg_key).encrypt(&mut msg);
        Ok(from_slice(&msg)?)
    }

    fn is_expired(&self) -> bool {
        self.created + Duration::seconds(self.ttl) < Utc::now()
    }

    fn into_msg(self) -> Option<Message> {
        if self.is_expired() {
            return None;
        }
        match &*self.action {
            "collectNow" => Some(Message::CollectNow),
            // we treat this the same as a collect, which will re-send the pubkey
            "rekeyAction" => Some(Message::CollectNow),
            "screenShot" => Some(Message::Screenshot),
            _ => {
                log::info!("got unsupported XMR action {:?}", self.action);
                None
            }
        }
    }
}

fn deserialize_datetime<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<DateTime<FixedOffset>, D::Error> {
    let s = <String as Deserialize>::deserialize(d)?;
    DateTime::parse_from_rfc3339(&s).map_err(|_| unimplemented!())
}

fn decrypt_private_key(enc_key: &[u8], private_key: &RsaPrivateKey) -> Result<Vec<u8>> {
    let padding = rsa::PaddingScheme::new_pkcs1v15_encrypt();
    let dec_data = private_key.decrypt(padding, &enc_key).context("failed to decrypt PK")?;
    Ok(dec_data)
}

#[test]
fn test_decrypt() {
    let pem = "-----BEGIN RSA PRIVATE KEY-----
MIICXAIBAAKBgQDJg84myV3VE+v53gQKVbX+6pQrveSfZTcs/a3mikxhXO32peqh
OP2namgoixfBBwK6wzRjRzOHdsB4yQPTMRTZIsipTYHyIqYl5/6AxoRGAsjZtmaB
MNsxrBxMCGlWEKLPwSCecT8EbCrfl3GArf56SEglxDRyx7pDRRnAihPgMQIBEQKB
gAQ7xwUeC6blhxvWaX8kOIeBs4QlVXmrABVh1Wa5wzfTs0BXYoJPt+IsL11bH7E7
TpQO23QaPD4Ba03U5TCJotumgDf0zIfVx5p7GrpK4oqI4o+PX7gWCzurXaqmQiYq
CfZCCeHF+Z2KV2OmhXq3tvlx8Ne4gOiZ65K2vNhNiAEZAkEA1wAyT/hFPUoDnqYD
UfRJEQM1XyRxa0MTkUJh4UO+WCp+d2OtEuydMUdfSu9oGPUNPsMaXr3SzsE8rhp8
1iXB1QJBAO/xQqxO0YvYnDJgQFTXB34Lv66pCHkbBddvYnByfxqeIQJM9o61grUK
LCLjrZ9qPqa87xcYLPP4i8/iPuMKtu0CQQCXw+dHghLB2eRv/LcMrG/PxgeOdBPT
PmgqTPnML9GnpYZyZHoredhfBTQ05Tpr+EWVtuVwDYW/Hv2oErJ5C5fhAkEAm0HB
usmWpchlEYmTCbhQJGH0gBMFe4n0uJNd0EoWAioVW9dyXFdUk0LRQ8B/ZyahAnpA
WjzRywo8WVYosQbu1QJBAIK8lUC6fBRr2ElLltNV/cmR2To5rUYSQJJB9rDw9Inv
cwFD2YnuxuF9szIeWPTmHUl6aXRIByuKNexbHqTeNhY=
-----END RSA PRIVATE KEY-----
";
    use rsa::pkcs1::FromRsaPrivateKey;
    let privkey = rsa::RsaPrivateKey::from_pkcs1_pem(pem).unwrap();
    let msg = JsonMessage::new(&privkey,
                               b"uKgfpneak5Qx5vppLlJZEEcFQ5Y/xrk45ysmnsIVQGvndFR0R86pPRRDPxvqSBgCDb\
                                 4xInqC8fQLApEzEjULL4QwERycgfHWMY+KSAEDjaS2/3IvSUPa+XYZVZssC/jddIar\
                                 ZvqHdfylHqm1IiL6Tgaps05BYeyDYynRmngW8NM=",
                               b"TOwhZC5mz2N0GoQvUDXsXVDfC3A6Ov5I+raxOsBvvhOLgPFlpz2VxWTsvq5TX8JJ/b\
                                 gCSdfpe5DTA0bEvwXzDst1KtGjK1Nvdg==").unwrap();
    assert_eq!(msg.action, "screenShot");
}
