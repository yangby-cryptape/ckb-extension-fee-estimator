use std::{net::SocketAddr, str::FromStr as _};

use ckb_jsonrpc_types as rpc;
use futures::{future, FutureExt as _, SinkExt as _, StreamExt as _, TryStreamExt as _};
use jsonrpc_core_client::{transports::duplex, RpcError as JsonRpcRpcError};
use jsonrpc_derive::rpc;
use jsonrpc_server_utils::{codecs::StreamCodec, tokio_util::codec::Decoder as _};
use tokio::net::TcpStream;

use crate::{
    error::{Error, Result},
    shared::Shared,
    types,
};

pub(crate) struct Subscriber;

#[rpc(client)]
pub trait SubscriptionRpc {
    type Metadata;

    #[pubsub(subscription = "subscribe", subscribe, name = "subscribe")]
    fn subscribe(&self, meta: Self::Metadata, subscriber: Subscriber<String>, topic: rpc::Topic);
    #[pubsub(subscription = "subscribe", unsubscribe, name = "unsubscribe")]
    fn unsubscribe(&self, meta: Option<Self::Metadata>, id: SubscriptionId) -> Result<bool>;
}

impl Subscriber {
    pub(crate) fn initialize(addr: SocketAddr, shared: Shared) -> Result<Self> {
        log::trace!("initialize a subscriber to synchronize transactions ...");
        let fut_conn = TcpStream::connect(&addr).map(|stream| {
            let local_addr = stream.as_ref().unwrap().local_addr().unwrap();
            log::trace!("successfully connect via {}", local_addr);
            stream
        });
        let stream = shared
            .runtime()
            .block_on(fut_conn)
            .map_err(Error::subscriber)?;
        let (sink, stream) = StreamCodec::stream_incoming().framed(stream).split();
        let sink = sink.sink_map_err(|err| JsonRpcRpcError::Other(Box::new(err)));
        let stream = stream
            .map_err(|err| JsonRpcRpcError::Other(Box::new(err)))
            .take_while(|x| future::ready(x.is_ok()))
            .map(|x| x.expect("Stream is closed upon first error."));
        let (rpc_client, sender) = duplex(Box::pin(sink), Box::pin(stream));
        let client = gen_client::Client::from(sender);
        let subscribe_tip_block = {
            let shared = shared.clone();
            let mut stream = client
                .subscribe(rpc::Topic::NewTipBlock)
                .map_err(Error::subscriber)?;
            async move {
                while let Some(res) = stream.next().await {
                    if let Ok(s) = res {
                        Self::handle_tip_block(shared.clone(), s);
                    } else {
                        log::error!("tip_block stream return error");
                    }
                }
            }
        };
        let subscribe_transaction = {
            let shared = shared.clone();
            let mut stream = client
                .subscribe(rpc::Topic::NewTransaction)
                .map_err(Error::subscriber)?;
            async move {
                while let Some(res) = stream.next().await {
                    if let Ok(s) = res {
                        Self::handle_transaction(shared.clone(), s);
                    } else {
                        log::error!("transaction stream return error");
                    }
                }
            }
        };
        shared.runtime().spawn(rpc_client);
        shared.runtime().spawn(subscribe_tip_block);
        shared.runtime().spawn(subscribe_transaction);
        let client = Subscriber;
        Ok(client)
    }

    fn handle_tip_block(shared: Shared, block: String) {
        log::trace!("receive tip block");
        if let Ok(block) = types::Block::from_str(&block) {
            log::trace!(">>> tip block#{} {:#x}", block.number(), block.hash());
            shared.commit_block(block);
        } else {
            log::error!("failed to deserialize tip block");
        }
    }

    fn handle_transaction(shared: Shared, tx: String) {
        log::trace!("receive transaction");
        if let Ok(tx) = types::Transaction::from_str(&tx) {
            log::trace!(">>> transaction {:#x}", tx.hash());
            shared.submit_transaction(tx);
        } else {
            log::error!("failed to deserialize transaction");
        }
    }
}
