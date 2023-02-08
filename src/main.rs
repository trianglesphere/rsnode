#![allow(dead_code)]

use dotenv::dotenv;
use ethers_core::{
	abi::AbiDecode,
	types::{Address, Block, Log, Transaction, TransactionReceipt, H128, H256},
};
use ethers_providers::{Http, Middleware, Provider};
use eyre::Result;
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;
use tokio::runtime::Runtime;

struct Client {
	provider: Provider<Http>,
	rt: Runtime,
}

#[derive(Serialize, Deserialize, Debug)]
struct BlockWithReceipts {
	block: Block<Transaction>,
	receipts: Vec<TransactionReceipt>,
}

impl Client {
	pub fn new(url: &str) -> Result<Self> {
		let provider = Provider::<Http>::try_from(url)?;
		let rt = tokio::runtime::Builder::new_current_thread().enable_all().build()?;

		Ok(Client { rt, provider })
	}

	fn get_transaction_receipt(&self, transaction_hash: H256) -> Result<TransactionReceipt> {
		let receipt = self.rt.block_on(self.provider.get_transaction_receipt(transaction_hash))?;

		receipt.ok_or(eyre::eyre!("did not find the receipt"))
	}

	pub fn get_block_with_receipts(&self, hash: H256) -> Result<BlockWithReceipts> {
		let block =
			self.rt.block_on(self.provider.get_block_with_txs(hash))?
				.ok_or(eyre::eyre!("did not find the block"))?;

		let mut receipts = Vec::new();

		for tx in block.transactions.iter() {
			let receipt = self.get_transaction_receipt(tx.hash)?;
			receipts.push(receipt)
		}

		Ok(BlockWithReceipts { block, receipts })
	}

	// pub fn get_head_block(&self) -> Result<Block<TxHash>, Box<dyn Error>> {
	// 	self.provider.get_block(block_hash_or_number)
	// }
}

// ConfigUpdateEventABI      = "ConfigUpdate(uint256,uint8,bytes)"
// ConfigUpdateEventABIHash  = crypto.Keccak256Hash([]byte(ConfigUpdateEventABI))
// ConfigUpdateEventVersion0 = common.Hash{}

struct SystemConfig {
	batcher_addr: Address,
	overhead: H256,
	scalar: H256,
	gas_limit: u64,
}

fn system_config_from_receipts(receipts: Vec<TransactionReceipt>, prev: SystemConfig) -> SystemConfig {
	let l1_system_config_addr = Address::decode_hex("").unwrap();
	let config_update_abi = H256::decode_hex("1d2b0bda21d56b8bd12d4f94ebacffdfb35f5e226f84b461103bb8beab6353be").unwrap();
	let _logs: Vec<&Log> = receipts
		.iter()
		.filter(|r| r.status == Some(1.into()))
		.flat_map(|r| r.logs.iter())
		.filter(|l| l.address == l1_system_config_addr)
		.filter(|l| l.topics.len() > 1 && l.topics[0] == config_update_abi)
		.collect();
	return prev;
}

fn main() -> Result<()> {
	// Load environment variables from local ".env" file
	dotenv().ok();

	let provider = std::env::var("RPC")?;
	let provider = Client::new(&provider)?;

	let hash = H256::decode_hex("0xee9dd94ebc06b50d5d5c0f72299a3cc56737e459ce41ddb44f0411870f86b1a3")?;

	let block = provider.get_block_with_receipts(hash)?;
	println!("Got block: {}", serde_json::to_string_pretty(&block)?);

	Ok(())
}
