use std::{net::SocketAddr, result::Result as StdResult, str::FromStr as _};

use futures::compat::Future01CompatExt as _;
use jsonrpc_core::futures::{Future as _, IntoFuture as _, Sink as _, Stream as _};
use jsonrpc_core_client::{transports::duplex, RpcError as JsonRpcRpcError};
use jsonrpc_derive::rpc;
use jsonrpc_server_utils::{
    codecs::StreamCodec,
    tokio::{codec::Decoder, net::TcpStream},
};

use crate::{
    error::{Error, Result},
    patches::Topic,
    shared::Shared,
    types,
};

pub(crate) struct Subscriber;

#[rpc(client)]
pub trait SubscriptionRpc {
    type Metadata;

    #[pubsub(subscription = "subscribe", subscribe, name = "subscribe")]
    fn subscribe(&self, meta: Self::Metadata, subscriber: Subscriber<String>, topic: Topic);
    #[pubsub(subscription = "subscribe", unsubscribe, name = "unsubscribe")]
    fn unsubscribe(&self, meta: Option<Self::Metadata>, id: SubscriptionId) -> Result<bool>;
}

impl Subscriber {
    pub(crate) fn initialize(addr: SocketAddr, shared: Shared) -> Result<Self> {
        log::trace!("initialize a subscriber to synchronize transactions ...");
        let fut_conn = TcpStream::connect(&addr).map(|stream| {
            log::trace!("successfully connect via {}", stream.local_addr().unwrap());
            stream
        });
        let stream = shared
            .runtime()
            .block_on(fut_conn.compat())
            .map_err(Error::subscriber)?;
        let (sink, stream) = StreamCodec::stream_incoming().framed(stream).split();
        let sink = sink.sink_map_err(|e| JsonRpcRpcError::Other(e.into()));
        let stream = stream.map_err(|e| JsonRpcRpcError::Other(e.into()));
        let (rpc_client, sender) = duplex(sink, stream);
        let client = gen_client::Client::from(sender);
        let subscribe_tip_block = {
            let shared = shared.clone();
            client
                .subscribe(Topic::NewTipBlock)
                .map(move |stream| {
                    shared.runtime().spawn(
                        stream
                            .for_each(move |s| Self::handle_tip_block(shared.clone(), s))
                            .into_future()
                            .compat(),
                    )
                })
                .map_err(|_| ())
                .compat()
        };
        let subscribe_transaction = {
            let shared = shared.clone();
            client
                .subscribe(Topic::NewTransaction)
                .map(move |stream| {
                    shared.runtime().spawn(
                        stream
                            .for_each(move |s| Self::handle_transaction(shared.clone(), s))
                            .into_future()
                            .compat(),
                    )
                })
                .map_err(|_| ())
                .compat()
        };
        shared.runtime().spawn(rpc_client.map_err(|_| ()).compat());
        shared.runtime().spawn(subscribe_tip_block);
        shared.runtime().spawn(subscribe_transaction);
        let client = Subscriber;
        Ok(client)
    }

    fn handle_tip_block(shared: Shared, block: String) -> StdResult<(), JsonRpcRpcError> {
        log::trace!("receive tip block");
        if let Ok(block) = types::Block::from_str(&block) {
            log::trace!(">>> tip block {:#x}", block.hash());
            shared.commit_block(block);
        } else {
            log::warn!("failed to deserialize tip block");
        }
        Ok(())
    }

    fn handle_transaction(shared: Shared, tx: String) -> StdResult<(), JsonRpcRpcError> {
        log::trace!("receive transaction");
        if let Ok(tx) = types::Transaction::from_str(&tx) {
            log::trace!(">>> transaction {:#x}", tx.hash());
            shared.submit_transaction(tx);
        } else {
            log::warn!("failed to deserialize transaction");
        }
        Ok(())
    }
}
