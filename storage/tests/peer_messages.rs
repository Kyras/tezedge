// Copyright (c) SimpleStaking and Tezedge Contributors
// SPDX-License-Identifier: MIT

use storage::{
    tests_common::TmpStorage,
    persistent::peer_messages::{
        PeerMessages, Recorder,
    },
};
use failure::Error;
use tezos_messages::p2p::encoding::peer::PeerMessage;

#[test]
fn create_replay() -> Result<(), Error> {
    println!("BP 0");
    let tmp_storage = TmpStorage::create("__peer_messages:create_storage")
        .expect("failed to create temporary torage");
    println!("BP 1");
    let kv = tmp_storage.storage().kv();
    println!("BP 2");
    let mut msg = PeerMessages::new(kv);
    println!("BP 3");
    let ts = msg.record(PeerMessage::Bootstrap)?.expect("failed to store temporary message");
    println!("BP 4");
    let replay = msg.replay()?;
    println!("BP 5");
    for (k, v) in replay {
        let (k, v) = (k?, v?);
        assert_eq!(k, ts);
        assert_eq!(v, PeerMessage::Bootstrap);
    }
    println!("BP 6");
    Ok(())
}