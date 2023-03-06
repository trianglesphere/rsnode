use core::types::H256;
use reth_rlp::Encodable;
use std::{collections::HashMap, fmt::Debug, iter::zip};

#[cfg(test)]
mod mpt_test;

#[derive(Debug)]
pub struct MPT {
	root: Node,
	db: HashMap<H256, Vec<u8>>,
}

#[derive(Debug)]
enum Node {
	Empty,
	Branch(BranchNode),
	Extension(ExtensionNode),
	Value(ValueNode),
}

struct ValueNode {
	value: Vec<u8>,
	hash: Option<H256>,
}

impl ValueNode {
	fn new(value: Vec<u8>) -> Self {
		Self { value, hash: None }
	}
}

impl From<Vec<u8>> for ValueNode {
	fn from(value: Vec<u8>) -> Self {
		ValueNode::new(value)
	}
}

impl From<ValueNode> for Node {
	fn from(value: ValueNode) -> Self {
		Node::Value(value)
	}
}

impl Debug for ValueNode {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_fmt(format_args!("{:x?}\n", &self.value))?;
		f.write_fmt(format_args!("hash: {:#?}", self.hash))
	}
}

struct ExtensionNode {
	nibbles: Vec<u8>,
	child: Box<Node>,
	hash: Option<H256>,
}

impl ExtensionNode {
	fn new(nibbles: Vec<u8>, child: Node) -> Self {
		Self {
			nibbles,
			child: Box::new(child),
			hash: None,
		}
	}
}

impl From<ExtensionNode> for Node {
	fn from(value: ExtensionNode) -> Self {
		Node::Extension(value)
	}
}

impl ExtensionNode {
	fn compact(&self) -> Vec<u8> {
		let extension = match *self.child {
			Node::Empty => panic!("Cannot point to an empty node in an extension"),
			Node::Extension(..) => panic!("Cannot point to an extension node in an extension node"),
			Node::Value(..) => false,
			Node::Branch(..) => true,
		};
		nibbles_to_compact(&self.nibbles, extension)
	}
}

impl Debug for ExtensionNode {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_fmt(format_args!("nibbles: {:x?}\n", &self.nibbles))?;
		f.write_fmt(format_args!("compact: {:x?}\n", self.compact()))?;
		f.write_fmt(format_args!("child: {:#?}", self.child))?;
		f.write_fmt(format_args!("hash: {:#?}", self.hash))
	}
}

#[derive(Default)]
struct BranchNode {
	children: [Box<Node>; 16],
	branch_value: Option<ValueNode>,
	hash: Option<H256>,
}

impl BranchNode {
	// inserts adds a key/value to a full node as either a sub-node or as a value.
	// It returns none if there is an error.
	pub fn insert(mut self, nibbles: &[u8], value: Vec<u8>) -> Option<Self> {
		if nibbles.is_empty() {
			if self.branch_value.is_some() {
				// TODO: Error: Cannot double insert into a branch node.
				return None;
			}
			self.branch_value = Some(value.into());
		} else {
			let i = nibbles[0] as usize;
			*self.children[i] = std::mem::take(&mut self.children[i]).insert(&nibbles[1..], value);
		};
		Some(self)
	}

	// new_with_node creates a new branch node that contains a given child node.
	pub fn new_with_node(key: u8, node: Box<Node>) -> Self {
		let mut branch_node = BranchNode::default();
		branch_node.children[key as usize] = node;
		branch_node
	}

	// new_with_value creates a new branch node that contains the given value.
	pub fn new_with_value(value: ValueNode) -> Self {
		let mut branch_node = BranchNode::default();
		branch_node.branch_value = Some(value);
		branch_node
	}
}

impl From<BranchNode> for Node {
	fn from(value: BranchNode) -> Self {
		Node::Branch(value)
	}
}

impl Debug for BranchNode {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		// TODO: Use f.alternate()
		for (i, v) in self.children.iter().enumerate() {
			f.write_fmt(format_args!("{i:x}: {:#?}\n", v))?;
		}
		f.write_fmt(format_args!("value: {:#?}", self.branch_value))?;
		f.write_fmt(format_args!("hash: {:#?}", self.hash))
	}
}

impl Default for Node {
	fn default() -> Self {
		Self::Empty
	}
}

impl Node {
	fn new(nibbles: &[u8], value: Vec<u8>) -> Self {
		if nibbles.is_empty() {
			ValueNode::new(value).into()
		} else {
			ExtensionNode::new(nibbles.to_owned(), ValueNode::new(value).into()).into()
		}
	}

	fn new_with_node(nibbles: &[u8], child: Box<Node>) -> Box<Self> {
		if nibbles.is_empty() {
			child
		} else {
			Box::new(ExtensionNode::new(nibbles.to_owned(), *child).into())
		}
	}

	fn insert(self, nibbles: &[u8], value: Vec<u8>) -> Self {
		match self {
			Node::Empty => Node::new(nibbles, value),
			Node::Branch(node) => node.insert(nibbles, value).unwrap().into(),
			Node::Extension(node) => {
				let (common, new_nibbles, old_nibbles) = match_paths(nibbles, &node.nibbles);
				if new_nibbles.is_empty() && old_nibbles.is_empty() {
					panic!("Paths cannot be the same");
				}
				// Inserting here will alwasy create branch node.
				// Turn the existing node into that branch node then insert the new value.
				let branch_node = if old_nibbles.is_empty() {
					match *node.child {
						Node::Empty => panic!("Cannot point to an empty node in an extension"),
						Node::Extension(..) => panic!("Cannot point to an extension node in an extension node"),
						Node::Value(child) => BranchNode::new_with_value(child),
						Node::Branch(child) => child,
					}
				} else {
					let child = Node::new_with_node(&old_nibbles[1..], node.child);
					BranchNode::new_with_node(old_nibbles[0], child)
				}
				.insert(new_nibbles, value)
				.unwrap();
				// Create an extension node based on the common part if needed.
				if common.is_empty() {
					branch_node.into()
				} else {
					ExtensionNode::new(common, branch_node.into()).into()
				}
			}
			Node::Value(..) => panic!("Cannot insert into a value node"),
		}
	}

	fn hash(&self) -> H256 {
		todo!()
	}
}

fn match_paths<'a, 'b>(key: &'a [u8], path: &'b [u8]) -> (Vec<u8>, &'a [u8], &'b [u8]) {
	let mut common = Vec::new();
	for (a, b) in zip(key, path) {
		if a == b {
			common.push(*a)
		} else {
			break;
		}
	}
	let i = common.len();
	(common, &key[i..], &path[i..])
}

impl MPT {
	pub fn new() -> Self {
		MPT {
			root: Node::Empty,
			db: HashMap::default(),
		}
	}
	pub fn hash(&self) -> H256 {
		todo!()
	}

	pub fn insert(&mut self, k: Vec<u8>, v: Vec<u8>) {
		let k = bytes_to_nibbles(&k);
		let root = std::mem::take(&mut self.root);
		self.root = root.insert(&k, v);
	}
}

pub fn bytes_to_nibbles(key: &[u8]) -> Vec<u8> {
	let mut out = Vec::new();
	for byte in key {
		out.push(byte >> 4);
		out.push(byte & 0x0f);
	}
	out
}

pub fn nibbles_to_compact(nibbles: &[u8], extension: bool) -> Vec<u8> {
	let mut key = nibbles;
	let mut out = Vec::new();
	let even = key.len() % 2 == 0;
	let mut first = match (extension, even) {
		(true, true) => 0,
		(true, false) => 1,
		(false, true) => 2,
		(false, false) => 3,
	} << 4;
	if !key.is_empty() && !even {
		first = first | key[0];
		key = &key[1..];
	}
	out.push(first);
	for a in key.chunks_exact(2) {
		out.push(a[0] << 4 | a[1]);
	}
	out
}

pub fn compact_to_nibbles(compact: &[u8]) -> (Vec<u8>, bool) {
	let (extension, even) = match compact[0] >> 4 {
		0 => (true, true),
		1 => (true, false),
		2 => (false, true),
		3 => (false, false),
		_ => panic!("out of range"),
	};
	let mut nibbles = Vec::new();
	if !even {
		nibbles.push(compact[0] & 0x0f);
	}
	for b in &compact[1..] {
		nibbles.push(b >> 4);
		nibbles.push(b & 0x0f);
	}
	(nibbles, extension)
}

// pub enum HashNode {
// 	Empty,
// 	Branch { children: [H256; 17] },
// 	Leaf { path: Vec<u8>, value: H256 },
// 	Extension { path: Vec<u8>, value: H256 },
// }

// #[derive(Default)]
// pub struct BranchNode {
// 	pub children: [H256; 17],
// }

// impl Encodable for BranchNode {
// 	fn encode(&self, out: &mut dyn reth_rlp::BufMut) {
// 		reth_rlp::encode_list(&self.children, out)
// 	}
// }

// #[derive(Default)]
// pub struct Leaf {
// 	pub children: [Vec<u8>; 2],
// }

// impl Encodable for Leaf {
// 	fn encode(&self, out: &mut dyn reth_rlp::BufMut) {
// 		reth_rlp::encode_list::<Vec<u8>, _>(&self.children, out)
// 	}
// }

// #[derive(Default)]
// pub struct Extension {
// 	pub children: [Vec<u8>; 2],
// }

// impl Encodable for Extension {
// 	fn encode(&self, out: &mut dyn reth_rlp::BufMut) {
// 		reth_rlp::encode_list::<Vec<u8>, _>(&self.children, out)
// 	}
// }
