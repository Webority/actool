//! BOM (Bill of Materials) file writer.
//!
//! BOM is Apple's container format used for `.car` files and `.pkg`
//! installers. All structures are big-endian.

use indexmap::IndexMap;
use std::path::Path;

#[cfg(test)]
use byteorder::BigEndian;

pub struct BomWriter {
    blocks: Vec<Vec<u8>>,
    named_blocks: IndexMap<String, usize>,
    inline_key_size: Option<usize>,
}

impl BomWriter {
    pub fn new() -> Self {
        Self {
            blocks: vec![Vec::new()], // block 0 is always empty
            named_blocks: IndexMap::new(),
            inline_key_size: None,
        }
    }

    /// Set the fixed key size for subsequent `add_tree` calls. When set, the
    /// leaf node reserves `n_entries * key_size` zero bytes after the
    /// block_size padding, matching Apple's RENDITIONS leaf layout. Set back
    /// to `None` before adding variable-key trees (FACETKEYS, APPEARANCEKEYS).
    pub fn set_inline_key_size(&mut self, size: Option<usize>) {
        self.inline_key_size = size;
    }

    pub fn add_block(&mut self, data: Vec<u8>) -> usize {
        let idx = self.blocks.len();
        self.blocks.push(data);
        idx
    }

    pub fn add_named_block(&mut self, name: &str, data: Vec<u8>) -> usize {
        let idx = self.add_block(data);
        self.named_blocks.insert(name.to_string(), idx);
        idx
    }

    pub fn add_tree(
        &mut self,
        name: &str,
        entries: &[(Vec<u8>, Vec<u8>)],
        block_size: u32,
    ) -> usize {
        // Entry format: value_block_idx(4) + key_block_idx(4) = 8 bytes
        // Node header: isLeaf(2) + count(2) + forward(4) + backward(4) = 12 bytes
        let max_per_node = ((block_size as usize) - 12) / 8;

        if entries.is_empty() {
            let mut node = Vec::with_capacity(block_size as usize);
            node.extend_from_slice(&1u16.to_be_bytes());
            node.extend_from_slice(&0u16.to_be_bytes());
            node.extend_from_slice(&0i32.to_be_bytes());
            node.extend_from_slice(&0u32.to_be_bytes());
            node.resize(block_size as usize, 0);
            let node_idx = self.add_block(node);
            let tree_hdr =
                build_tree_header(node_idx, block_size, 0, 0, self.tree_key_size());
            let tree_idx = self.add_block(tree_hdr);
            self.named_blocks.insert(name.to_string(), tree_idx);
            return tree_idx;
        }

        let leaf_batches: Vec<&[(Vec<u8>, Vec<u8>)]> =
            entries.chunks(max_per_node).collect();

        if leaf_batches.len() == 1 {
            let node = self.build_leaf_node(leaf_batches[0], 0, 0, block_size);
            let node_idx = self.add_block(node);
            let tree_hdr = build_tree_header(
                node_idx,
                block_size,
                entries.len() as u32,
                0,
                self.tree_key_size(),
            );
            let tree_idx = self.add_block(tree_hdr);
            self.named_blocks.insert(name.to_string(), tree_idx);
            return tree_idx;
        }

        // Reserve leaf indices before we know their contents.
        let leaf_indices: Vec<usize> = leaf_batches
            .iter()
            .map(|_| self.add_block(vec![0]))
            .collect();

        for (i, batch) in leaf_batches.iter().enumerate() {
            let fwd = if i + 1 < leaf_indices.len() {
                leaf_indices[i + 1] as u32
            } else {
                0
            };
            let bwd = if i > 0 { leaf_indices[i - 1] as u32 } else { 0 };
            let node = self.build_leaf_node(batch, fwd, bwd, block_size);
            self.blocks[leaf_indices[i]] = node;
        }

        let mut internal = Vec::new();
        internal.extend_from_slice(&0u16.to_be_bytes()); // isLeaf = 0
        internal.extend_from_slice(&((leaf_indices.len() - 1) as u16).to_be_bytes());
        internal.extend_from_slice(&0u32.to_be_bytes());
        internal.extend_from_slice(&0u32.to_be_bytes());
        internal.extend_from_slice(&(leaf_indices[0] as u32).to_be_bytes());
        for i in 1..leaf_indices.len() {
            let first_key = leaf_batches[i][0].0.clone();
            let key_idx = self.add_block(first_key);
            internal.extend_from_slice(&(key_idx as u32).to_be_bytes());
            internal.extend_from_slice(&(leaf_indices[i] as u32).to_be_bytes());
        }
        let internal_idx = self.add_block(internal);
        let tree_hdr = build_tree_header(
            internal_idx,
            block_size,
            entries.len() as u32,
            0,
            self.tree_key_size(),
        );
        let tree_idx = self.add_block(tree_hdr);
        self.named_blocks.insert(name.to_string(), tree_idx);
        tree_idx
    }

    fn tree_key_size(&self) -> u32 {
        match self.inline_key_size {
            Some(n) => n as u32,
            None => 0xFFFFFFFF,
        }
    }

    fn build_leaf_node(
        &mut self,
        entries: &[(Vec<u8>, Vec<u8>)],
        forward: u32,
        backward: u32,
        block_size: u32,
    ) -> Vec<u8> {
        let mut node = Vec::new();
        node.extend_from_slice(&1u16.to_be_bytes()); // isLeaf
        node.extend_from_slice(&(entries.len() as u16).to_be_bytes());
        node.extend_from_slice(&forward.to_be_bytes());
        node.extend_from_slice(&backward.to_be_bytes());
        let mut key_blob: Vec<u8> = Vec::new();
        for (key_data, value_data) in entries {
            let val_idx = self.add_block(value_data.clone());
            let key_idx = self.add_block(key_data.clone());
            node.extend_from_slice(&(val_idx as u32).to_be_bytes());
            node.extend_from_slice(&(key_idx as u32).to_be_bytes());
            if self.inline_key_size.is_some() {
                key_blob.extend_from_slice(key_data);
            }
        }
        // For fixed-key trees (RENDITIONS, APPEARANCEKEYS), Apple inlines
        // each rendition key immediately after the entries — CUICatalog
        // reads keys from this packed region rather than chasing key block
        // pointers, so the order and adjacency to entries matters. Without
        // this, `imagesWithName:` silently returns empty arrays even when
        // every separate key block is correct. A 4-byte zero gap separates
        // the entry table from the inline-key region.
        if !key_blob.is_empty() {
            node.extend_from_slice(&[0u8; 4]);
            node.extend_from_slice(&key_blob);
        }
        if (node.len() as u32) < block_size {
            node.resize(block_size as usize, 0);
        }
        // After the inline-key region the leaf is padded with zeros until
        // total length = block_size + n_entries * fixed_key_size.
        if let Some(ks) = self.inline_key_size {
            let target = block_size as usize + entries.len() * ks;
            if node.len() < target {
                node.resize(target, 0);
            }
        }
        node
    }

    pub fn add_raw_key_tree(
        &mut self,
        name: &str,
        entries: &[(u32, Vec<u8>)],
        block_size: u32,
    ) -> usize {
        if entries.is_empty() {
            let mut node = Vec::with_capacity(block_size as usize);
            node.extend_from_slice(&1u16.to_be_bytes());
            node.extend_from_slice(&0u16.to_be_bytes());
            node.extend_from_slice(&0u32.to_be_bytes());
            node.extend_from_slice(&0u32.to_be_bytes());
            node.resize(block_size as usize, 0);
            let node_idx = self.add_block(node);
            let tree_hdr = build_tree_header(node_idx, block_size, 0, 1, 0);
            let tree_idx = self.add_block(tree_hdr);
            self.named_blocks.insert(name.to_string(), tree_idx);
            return tree_idx;
        }

        let max_per_node = ((block_size as usize) - 12) / 8;
        let batches: Vec<&[(u32, Vec<u8>)]> =
            entries.chunks(max_per_node).collect();

        if batches.len() == 1 {
            let mut node = Vec::new();
            node.extend_from_slice(&1u16.to_be_bytes());
            node.extend_from_slice(&(entries.len() as u16).to_be_bytes());
            node.extend_from_slice(&0u32.to_be_bytes());
            node.extend_from_slice(&0u32.to_be_bytes());
            for (raw_key, value_data) in entries {
                let val_idx = self.add_block(value_data.clone());
                node.extend_from_slice(&(val_idx as u32).to_be_bytes());
                node.extend_from_slice(&(*raw_key).to_be_bytes());
            }
            if (node.len() as u32) < block_size {
                node.resize(block_size as usize, 0);
            }
            let node_idx = self.add_block(node);
            let tree_hdr =
                build_tree_header(node_idx, block_size, entries.len() as u32, 1, 0);
            let tree_idx = self.add_block(tree_hdr);
            self.named_blocks.insert(name.to_string(), tree_idx);
            return tree_idx;
        }

        let leaf_indices: Vec<usize> =
            batches.iter().map(|_| self.add_block(vec![0])).collect();

        for (i, batch) in batches.iter().enumerate() {
            let fwd = if i + 1 < leaf_indices.len() {
                leaf_indices[i + 1] as u32
            } else {
                0
            };
            let bwd = if i > 0 { leaf_indices[i - 1] as u32 } else { 0 };
            let mut node = Vec::new();
            node.extend_from_slice(&1u16.to_be_bytes());
            node.extend_from_slice(&(batch.len() as u16).to_be_bytes());
            node.extend_from_slice(&fwd.to_be_bytes());
            node.extend_from_slice(&bwd.to_be_bytes());
            for (raw_key, value_data) in *batch {
                let val_idx = self.add_block(value_data.clone());
                node.extend_from_slice(&(val_idx as u32).to_be_bytes());
                node.extend_from_slice(&(*raw_key).to_be_bytes());
            }
            if (node.len() as u32) < block_size {
                node.resize(block_size as usize, 0);
            }
            self.blocks[leaf_indices[i]] = node;
        }

        let mut internal = Vec::new();
        internal.extend_from_slice(&0u16.to_be_bytes());
        internal.extend_from_slice(&((leaf_indices.len() - 1) as u16).to_be_bytes());
        internal.extend_from_slice(&0u32.to_be_bytes());
        internal.extend_from_slice(&0u32.to_be_bytes());
        internal.extend_from_slice(&(leaf_indices[0] as u32).to_be_bytes());
        for i in 1..leaf_indices.len() {
            let first_key = batches[i][0].0;
            internal.extend_from_slice(&first_key.to_be_bytes());
            internal.extend_from_slice(&(leaf_indices[i] as u32).to_be_bytes());
        }
        let internal_idx = self.add_block(internal);
        let tree_hdr =
            build_tree_header(internal_idx, block_size, entries.len() as u32, 1, 0);
        let tree_idx = self.add_block(tree_hdr);
        self.named_blocks.insert(name.to_string(), tree_idx);
        tree_idx
    }

    pub fn write<P: AsRef<Path>>(&self, path: P) -> std::io::Result<()> {
        let buf = self.to_bytes();
        std::fs::write(path, buf)
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        const MIN_TABLE_SIZE: usize = 256;
        const FREELIST_SIZE: usize = 20;

        let header_size: u64 = 32;
        let mut current_offset: u64 = header_size;
        let num_used_blocks = self.blocks.len() as u32;

        let mut block_entries: Vec<(u32, u32)> = Vec::new();
        for (i, block_data) in self.blocks.iter().enumerate() {
            if i == 0 {
                block_entries.push((0, 0));
                continue;
            }
            if current_offset % 16 != 0 {
                current_offset += 16 - (current_offset % 16);
            }
            block_entries
                .push((current_offset as u32, block_data.len() as u32));
            current_offset += block_data.len() as u64;
        }
        while block_entries.len() < MIN_TABLE_SIZE {
            block_entries.push((0, 0));
        }

        let mut index_data = Vec::new();
        index_data.extend_from_slice(&(block_entries.len() as u32).to_be_bytes());
        for (off, len) in &block_entries {
            index_data.extend_from_slice(&(*off).to_be_bytes());
            index_data.extend_from_slice(&(*len).to_be_bytes());
        }
        index_data.extend(std::iter::repeat(0u8).take(FREELIST_SIZE));

        let mut vars_data = Vec::new();
        vars_data.extend_from_slice(&(self.named_blocks.len() as u32).to_be_bytes());
        for (name, block_idx) in &self.named_blocks {
            let name_bytes = name.as_bytes();
            vars_data.extend_from_slice(&(*block_idx as u32).to_be_bytes());
            vars_data.push(name_bytes.len() as u8);
            vars_data.extend_from_slice(name_bytes);
        }

        if current_offset % 16 != 0 {
            current_offset += 16 - (current_offset % 16);
        }
        let vars_offset = current_offset;
        let vars_length = vars_data.len() as u32;
        current_offset += vars_length as u64;

        if current_offset % 16 != 0 {
            current_offset += 16 - (current_offset % 16);
        }
        let index_offset = current_offset;
        let index_length = index_data.len() as u32;

        let mut out = Vec::new();
        out.extend_from_slice(b"BOMStore");
        out.extend_from_slice(&1u32.to_be_bytes()); // version
        out.extend_from_slice(&num_used_blocks.to_be_bytes());
        out.extend_from_slice(&(index_offset as u32).to_be_bytes());
        out.extend_from_slice(&index_length.to_be_bytes());
        out.extend_from_slice(&(vars_offset as u32).to_be_bytes());
        out.extend_from_slice(&vars_length.to_be_bytes());

        for (i, block_data) in self.blocks.iter().enumerate() {
            if i == 0 {
                continue;
            }
            let expected = block_entries[i].0 as u64;
            let pos = out.len() as u64;
            if pos < expected {
                let pad = (expected - pos) as usize;
                out.resize(out.len() + pad, 0);
            }
            out.extend_from_slice(block_data);
        }

        let pos = out.len() as u64;
        if pos < vars_offset {
            let pad = (vars_offset - pos) as usize;
            out.resize(out.len() + pad, 0);
        }
        out.extend_from_slice(&vars_data);

        let pos = out.len() as u64;
        if pos < index_offset {
            let pad = (index_offset - pos) as usize;
            out.resize(out.len() + pad, 0);
        }
        out.extend_from_slice(&index_data);

        out
    }
}

impl Default for BomWriter {
    fn default() -> Self {
        Self::new()
    }
}

fn build_tree_header(
    root_idx: usize,
    block_size: u32,
    n_paths: u32,
    flag: u8,
    key_size: u32,
) -> Vec<u8> {
    // "tree"(4) + version(4) + root(4) + block_size(4) + n_paths(4) +
    // flag(1) + key_size(4) + pad(4).
    // key_size is the per-entry inline key length: the actual size for
    // fixed-key trees (RENDITIONS), 0xFFFFFFFF for variable-key trees
    // (FACETKEYS, APPEARANCEKEYS), or 0 for raw-key trees (BITMAPKEYS).
    let mut hdr = Vec::new();
    hdr.extend_from_slice(b"tree");
    hdr.extend_from_slice(&1u32.to_be_bytes());
    hdr.extend_from_slice(&(root_idx as u32).to_be_bytes());
    hdr.extend_from_slice(&block_size.to_be_bytes());
    hdr.extend_from_slice(&n_paths.to_be_bytes());
    hdr.push(flag);
    hdr.extend_from_slice(&key_size.to_be_bytes());
    hdr.extend_from_slice(&0u32.to_be_bytes());
    hdr
}

#[cfg(test)]
mod tests {
    use super::*;
    use byteorder::ReadBytesExt;
    use std::io::Cursor;

    #[test]
    fn empty_bom() {
        let bom = BomWriter::new();
        let data = bom.to_bytes();
        assert_eq!(&data[..8], b"BOMStore");
        let mut c = Cursor::new(&data[8..]);
        assert_eq!(c.read_u32::<BigEndian>().unwrap(), 1); // version
        assert_eq!(c.read_u32::<BigEndian>().unwrap(), 1); // num blocks (just null)
    }

    #[test]
    fn single_named_block() {
        let mut bom = BomWriter::new();
        let idx = bom.add_named_block("HELLO", b"world data".to_vec());
        assert_eq!(idx, 1);
        let data = bom.to_bytes();
        assert_eq!(&data[..8], b"BOMStore");
        // The bytes "world data" must appear somewhere in the output
        assert!(data.windows(10).any(|w| w == b"world data"));
    }

    #[test]
    fn tree_single_leaf() {
        let mut bom = BomWriter::new();
        let entries = vec![(b"key1".to_vec(), b"val1".to_vec())];
        bom.add_tree("TREE", &entries, 4096);
        let data = bom.to_bytes();
        assert!(data.windows(4).any(|w| w == b"tree"));
        assert!(data.windows(4).any(|w| w == b"key1"));
        assert!(data.windows(4).any(|w| w == b"val1"));
    }

    #[test]
    fn raw_key_tree() {
        let mut bom = BomWriter::new();
        let entries: Vec<(u32, Vec<u8>)> =
            vec![(1, b"aaaa".to_vec()), (2, b"bbbb".to_vec())];
        bom.add_raw_key_tree("TBL", &entries, 1024);
        let data = bom.to_bytes();
        assert!(data.windows(4).any(|w| w == b"aaaa"));
        assert!(data.windows(4).any(|w| w == b"bbbb"));
    }

    #[test]
    fn named_block_ordering_preserved() {
        let mut bom = BomWriter::new();
        bom.add_named_block("FIRST", b"AAAA".to_vec());
        bom.add_named_block("SECOND", b"BBBB".to_vec());
        bom.add_named_block("THIRD", b"CCCC".to_vec());
        let data = bom.to_bytes();
        // Find the vars section by looking for the name bytes.
        let first = data
            .windows(5)
            .position(|w| w == b"FIRST")
            .expect("FIRST not found");
        let second = data
            .windows(6)
            .position(|w| w == b"SECOND")
            .expect("SECOND not found");
        let third = data
            .windows(5)
            .position(|w| w == b"THIRD")
            .expect("THIRD not found");
        assert!(first < second);
        assert!(second < third);
    }
}
