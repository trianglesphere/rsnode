use ethers_core::types::{Block, Transaction, TransactionReceipt, H256};
use ethers_providers::{Http, Middleware, Provider};
use eyre::Result;
use std::{collections::HashMap, convert::TryFrom};
use tokio::runtime::Runtime;

use crate::types::{self, *};

/// Client wraps a web3 provider to provide L1 pre-image oracle support.
#[derive(Debug)]
pub struct Client {
	/// The internal web3 provider
	pub provider: Provider<Http>,
	/// The client runtime
	pub rt: Runtime,
	/// Store of receipts from Receipt Root to Receipts
	pub receipts: HashMap<H256, Vec<TransactionReceipt>>,
	/// Store of transactions from Transaction Root to Transactions
	pub transactions: HashMap<H256, Vec<Transaction>>,
}

impl Client {
	/// Constructs a new client
	pub fn new(url: &str) -> Result<Self> {
		let provider = Provider::<Http>::try_from(url)?;
		let rt = tokio::runtime::Builder::new_current_thread().enable_all().build()?;

		Ok(Client {
			rt,
			provider,
			receipts: HashMap::new(),
			transactions: HashMap::new(),
		})
	}

	/// Gets a block header by block hash
	pub fn get_header(&self, hash: H256) -> Result<Header> {
		let block = self.get_block_with_txs(hash)?;
		let header = types::header_from_block(block)?;
		Ok(header)
	}

	/// Gets a block with transactions
	pub fn get_block_with_txs(&self, hash: H256) -> Result<Block<Transaction>> {
		let block =
			self.rt.block_on(self.provider.get_block_with_txs(hash))?
				.ok_or(eyre::eyre!("did not find the block"))?;
		Ok(block)
	}

	/// Gets a block with its receipts
	pub fn get_block_with_receipts(&mut self, hash: H256) -> Result<BlockWithReceipts> {
		let block = self.get_block_with_txs(hash)?;
		self.transactions.insert(block.transactions_root, block.transactions.clone());
		let receipts = self.get_receipts_by_transactions(&block.transactions)?;
		self.receipts.insert(block.receipts_root, receipts.clone());
		Ok(BlockWithReceipts { block, receipts })
	}

	/// Get receipts by the recipt root
	pub fn get_receipts_by_root(&self, root: H256) -> Result<Vec<TransactionReceipt>> {
		self.receipts
			.get(&root)
			.ok_or(eyre::eyre!("missing receipts for given root in internal store"))
			.cloned()
	}

	/// Get transaction receipts for a list of transactions
	fn get_receipts_by_transactions(&self, transactions: &[Transaction]) -> Result<Vec<TransactionReceipt>> {
		let mut receipts = Vec::new();
		for tx in transactions.iter() {
			let receipt = self.get_transaction_receipt(tx.hash)?;
			receipts.push(receipt)
		}

		Ok(receipts)
	}

	/// Gets a transaction receipt by transaction hash
	fn get_transaction_receipt(&self, transaction_hash: H256) -> Result<TransactionReceipt> {
		let receipt = self.rt.block_on(self.provider.get_transaction_receipt(transaction_hash))?;
		receipt.ok_or(eyre::eyre!("did not find the receipt"))
	}

	/// Get transactions by the transaction root
	pub fn get_transactions_by_root(&self, root: H256) -> Result<Vec<Transaction>> {
		self.transactions
			.get(&root)
			.ok_or(eyre::eyre!("missing transactions for given root in internal store"))
			.cloned()
	}
}
