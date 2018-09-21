use std::net::SocketAddr;

use consensus::Epoch;
use dataflow::prelude::*;
use dataflow::DomainBuilder;

/// Coordination-layer message wrapper; adds a mandatory `source` field to each message.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CoordinationMessage {
    /// The worker's `SocketAddr` from which this message was sent.
    pub source: SocketAddr,
    /// The epoch this message is associated with.
    pub epoch: Epoch,
    /// Message payload.
    pub payload: CoordinationPayload,
}

/// Coordination-layer message payloads.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum CoordinationPayload {
    /// Register a new worker.
    Register {
        /// Address of the worker.
        addr: SocketAddr,
        /// Address the worker will be listening on to serve reads.
        read_listen_addr: SocketAddr,
        /// Which log files are stored locally on the worker.
        log_files: Vec<String>,
    },
    /// Worker going offline.
    Deregister,
    /// Worker is still alive.
    Heartbeat,
    /// Assign a new domain for a worker to run.
    AssignDomain(DomainBuilder),
    /// Remove a running domain from a worker.
    RemoveDomain,
    /// Domain connectivity gossip.
    DomainBooted(DomainDescriptor),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DomainDescriptor {
    id: DomainIndex,
    shard: usize,
    addr: SocketAddr,
}

impl DomainDescriptor {
    pub fn new(id: DomainIndex, shard: usize, addr: SocketAddr) -> Self {
        DomainDescriptor { id, shard, addr }
    }

    pub fn domain(&self) -> DomainIndex {
        self.id
    }

    pub fn shard(&self) -> usize {
        self.shard
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }
}
