use futures::{future, stream::BoxStream, StreamExt};

use nimiq_block::{Block, MacroBlock};
use nimiq_blockchain_interface::{
    AbstractBlockchain, BlockchainError, BlockchainEvent, ChainInfo, Direction, ForkEvent,
};
use nimiq_genesis::NetworkId;
use nimiq_hash::Blake2bHash;
use nimiq_primitives::slots::{Validator, Validators};
use tokio_stream::wrappers::BroadcastStream;

use crate::blockchain::LightBlockchain;

/// Implements several basic methods for blockchains.
impl AbstractBlockchain for LightBlockchain {
    fn network_id(&self) -> NetworkId {
        self.network_id
    }

    fn now(&self) -> u64 {
        self.time.now()
    }

    fn head(&self) -> Block {
        self.head.clone()
    }

    fn macro_head(&self) -> MacroBlock {
        self.macro_head.clone()
    }

    fn election_head(&self) -> MacroBlock {
        self.election_head.clone()
    }

    fn current_validators(&self) -> Option<Validators> {
        self.current_validators.clone()
    }

    fn previous_validators(&self) -> Option<Validators> {
        unreachable!()
    }

    fn contains(&self, hash: &Blake2bHash, include_forks: bool) -> bool {
        match self.chain_store.get_chain_info(hash, false) {
            Ok(chain_info) => include_forks || chain_info.on_main_chain,
            Err(_) => false,
        }
    }

    fn get_block_at(&self, height: u32, include_body: bool) -> Result<Block, BlockchainError> {
        self.chain_store
            .get_chain_info_at(height, include_body)
            .map(|chain_info| chain_info.head)
    }

    fn get_block(&self, hash: &Blake2bHash, include_body: bool) -> Result<Block, BlockchainError> {
        self.chain_store
            .get_chain_info(hash, include_body)
            .map(|chain_info| chain_info.head.clone())
    }

    fn get_chain_info(
        &self,
        hash: &Blake2bHash,
        include_body: bool,
    ) -> Result<ChainInfo, BlockchainError> {
        self.chain_store.get_chain_info(hash, include_body).cloned()
    }

    fn get_slot_owner_at(
        &self,
        block_number: u32,
        offset: u32,
    ) -> Result<(Validator, u16), BlockchainError> {
        let vrf_entropy = self.get_block_at(block_number - 1, false)?.seed().entropy();

        let slot = self.get_proposer_at(block_number, offset, vrf_entropy)?;

        Ok((slot.validator, slot.number))
    }

    fn get_proposer_at(
        &self,
        block_number: u32,
        offset: u32,
        vrf_entropy: nimiq_vrf::VrfEntropy,
    ) -> Result<nimiq_primitives::slots::Slot, BlockchainError> {
        self.get_proposer_at(block_number, offset, vrf_entropy)
    }

    fn get_blocks(
        &self,
        start_block_hash: &Blake2bHash,
        count: u32,
        include_body: bool,
        direction: Direction,
    ) -> Result<Vec<Block>, BlockchainError> {
        self.chain_store
            .get_blocks(start_block_hash, count, direction, include_body)
    }

    /// Fetches a given number of macro blocks, starting at a specific block (by its hash).
    /// It can fetch only election macro blocks if desired.
    /// Returns None if given start_block_hash is not a macro block.
    fn get_macro_blocks(
        &self,
        start_block_hash: &Blake2bHash,
        count: u32,
        include_body: bool,
        direction: Direction,
        election_blocks_only: bool,
    ) -> Result<Vec<Block>, BlockchainError> {
        self.chain_store.get_macro_blocks(
            start_block_hash,
            count,
            direction,
            election_blocks_only,
            include_body,
        )
    }

    fn notifier_as_stream(&self) -> BoxStream<'static, BlockchainEvent> {
        BroadcastStream::new(self.notifier.subscribe())
            .filter_map(|x| future::ready(x.ok()))
            .boxed()
    }

    fn fork_notifier_as_stream(&self) -> BoxStream<'static, ForkEvent> {
        BroadcastStream::new(self.fork_notifier.subscribe())
            .filter_map(|x| future::ready(x.ok()))
            .boxed()
    }
}
