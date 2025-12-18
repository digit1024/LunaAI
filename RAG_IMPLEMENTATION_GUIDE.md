# Local Knowledge Base / RAG Implementation Guide

## 🎯 Goal
Implement a local knowledge base with RAG (Retrieval-Augmented Generation) capabilities similar to 5ire, using bge-m3 or similar embedding model.

## 📋 Architecture Overview

```
┌─────────────────┐
│  Document Input │ (PDF, DOCX, XLSX, PPTX, TXT, CSV)
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  Document Parser│ (Extract text, preserve structure)
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  Text Chunker   │ (Split into semantic chunks)
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Embedding Model │ (bge-m3 or similar - local)
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  Vector Store   │ (SQLite with embeddings)
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  RAG Retrieval  │ (Query → Embedding → Similarity Search)
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Context Injection│ (Top-k chunks → LLM context)
└─────────────────┘
```

## 🏗️ Implementation Steps

### Step 1: Add Dependencies

Add to `Cargo.toml`:

```toml
# Embedding model (choose one approach)

# Option A: ONNX Runtime (recommended for ease)
ort = "2.0"  # ONNX Runtime for Rust

# Option B: Candle (pure Rust, more control)
# candle-core = "0.3"
# candle-nn = "0.3"
# candle-transformers = "0.3"

# Document parsing
lopdf = "0.32"  # PDF
docx-rs = "0.1"  # DOCX
calamine = "0.24"  # XLSX, XLS
pptx-rs = "0.1"  # PPTX (or use python script via subprocess)

# Text processing
regex = "1.10"
unicode-segmentation = "1.11"  # For better text splitting

# Vector similarity (if not using SQLite)
# qdrant-client = "1.7"  # External vector DB (optional)
```

### Step 2: Create Knowledge Base Module Structure

```
src/storage/
├── knowledge_base.rs      # Main knowledge base interface
├── document_parser.rs      # Document parsing logic
├── chunking.rs            # Text chunking strategies
├── embeddings.rs          # Embedding model wrapper
└── rag_retrieval.rs       # RAG query and retrieval
```

### Step 3: Document Parser Implementation

```rust
// src/storage/document_parser.rs
use anyhow::Result;
use std::path::Path;

pub enum DocumentType {
    Pdf,
    Docx,
    Xlsx,
    Pptx,
    Txt,
    Csv,
}

pub struct ParsedDocument {
    pub content: String,
    pub metadata: DocumentMetadata,
    pub chunks: Vec<TextChunk>,
}

pub struct DocumentMetadata {
    pub file_name: String,
    pub file_type: DocumentType,
    pub page_count: Option<usize>,
    pub word_count: usize,
    pub created_at: i64,
}

pub struct TextChunk {
    pub content: String,
    pub chunk_index: usize,
    pub page_number: Option<usize>,
    pub section: Option<String>,
}

pub trait DocumentParser {
    fn parse(&self, path: &Path) -> Result<ParsedDocument>;
}

// PDF Parser
pub struct PdfParser;
impl DocumentParser for PdfParser {
    fn parse(&self, path: &Path) -> Result<ParsedDocument> {
        use lopdf::Document;
        
        let doc = Document::load(path)?;
        let mut content = String::new();
        
        for page_num in 1..=doc.get_pages().len() {
            if let Ok(text) = doc.extract_text(&[page_num]) {
                content.push_str(&text);
                content.push_str("\n\n");
            }
        }
        
        Ok(ParsedDocument {
            content,
            metadata: DocumentMetadata {
                file_name: path.file_name().unwrap().to_string_lossy().to_string(),
                file_type: DocumentType::Pdf,
                page_count: Some(doc.get_pages().len()),
                word_count: content.split_whitespace().count(),
                created_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_secs() as i64,
            },
            chunks: vec![], // Will be chunked separately
        })
    }
}

// DOCX Parser
pub struct DocxParser;
impl DocumentParser for DocxParser {
    fn parse(&self, path: &Path) -> Result<ParsedDocument> {
        use docx_rs::read_docx;
        use std::fs;
        
        let data = fs::read(path)?;
        let docx = read_docx(&data)?;
        
        let mut content = String::new();
        for paragraph in docx.document.body.paragraphs {
            if let Some(text) = paragraph.text {
                content.push_str(&text);
                content.push_str("\n");
            }
        }
        
        Ok(ParsedDocument {
            content,
            metadata: DocumentMetadata {
                file_name: path.file_name().unwrap().to_string_lossy().to_string(),
                file_type: DocumentType::Docx,
                page_count: None,
                word_count: content.split_whitespace().count(),
                created_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_secs() as i64,
            },
            chunks: vec![],
        })
    }
}

// XLSX Parser
pub struct XlsxParser;
impl DocumentParser for XlsxParser {
    fn parse(&self, path: &Path) -> Result<ParsedDocument> {
        use calamine::{open_workbook, Reader, Xlsx};
        
        let mut workbook: Xlsx<_> = open_workbook(path)?;
        let mut content = String::new();
        
        for sheet_name in workbook.sheet_names() {
            content.push_str(&format!("\n## Sheet: {}\n\n", sheet_name));
            
            if let Ok(range) = workbook.worksheet_range(&sheet_name) {
                for row in range.rows() {
                    let row_text: Vec<String> = row
                        .iter()
                        .map(|cell| cell.to_string())
                        .collect();
                    content.push_str(&row_text.join(" | "));
                    content.push_str("\n");
                }
            }
        }
        
        Ok(ParsedDocument {
            content,
            metadata: DocumentMetadata {
                file_name: path.file_name().unwrap().to_string_lossy().to_string(),
                file_type: DocumentType::Xlsx,
                page_count: None,
                word_count: content.split_whitespace().count(),
                created_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_secs() as i64,
            },
            chunks: vec![],
        })
    }
}

// Factory function
pub fn parse_document(path: &Path) -> Result<ParsedDocument> {
    let extension = path.extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_lowercase();
    
    match extension.as_str() {
        "pdf" => PdfParser.parse(path),
        "docx" => DocxParser.parse(path),
        "xlsx" | "xls" => XlsxParser.parse(path),
        "txt" => {
            let content = std::fs::read_to_string(path)?;
            Ok(ParsedDocument {
                content,
                metadata: DocumentMetadata {
                    file_name: path.file_name().unwrap().to_string_lossy().to_string(),
                    file_type: DocumentType::Txt,
                    page_count: None,
                    word_count: content.split_whitespace().count(),
                    created_at: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)?
                        .as_secs() as i64,
                },
                chunks: vec![],
            })
        }
        _ => Err(anyhow::anyhow!("Unsupported file type: {}", extension)),
    }
}
```

### Step 4: Text Chunking Strategy

```rust
// src/storage/chunking.rs
use unicode_segmentation::UnicodeSegmentation;

pub struct ChunkingStrategy {
    pub chunk_size: usize,
    pub chunk_overlap: usize,
}

impl Default for ChunkingStrategy {
    fn default() -> Self {
        Self {
            chunk_size: 500,  // ~500 tokens
            chunk_overlap: 50,  // 50 token overlap
        }
    }
}

pub fn chunk_text(text: &str, strategy: &ChunkingStrategy) -> Vec<String> {
    // Simple sentence-based chunking
    let sentences: Vec<&str> = text
        .split_inclusive(&['.', '!', '?', '\n'])
        .collect();
    
    let mut chunks = Vec::new();
    let mut current_chunk = String::new();
    
    for sentence in sentences {
        let sentence_len = sentence.split_whitespace().count();
        let current_len = current_chunk.split_whitespace().count();
        
        if current_len + sentence_len > strategy.chunk_size && !current_chunk.is_empty() {
            chunks.push(current_chunk.trim().to_string());
            
            // Overlap: keep last N words
            let words: Vec<&str> = current_chunk.split_whitespace().collect();
            let overlap_start = words.len().saturating_sub(strategy.chunk_overlap);
            current_chunk = words[overlap_start..].join(" ");
            current_chunk.push(' ');
        }
        
        current_chunk.push_str(sentence);
    }
    
    if !current_chunk.trim().is_empty() {
        chunks.push(current_chunk.trim().to_string());
    }
    
    chunks
}
```

### Step 5: Embedding Model Wrapper

```rust
// src/storage/embeddings.rs
use anyhow::Result;
use ort::{Session, Value, Tensor};

pub struct EmbeddingModel {
    session: Session,
    dimension: usize,
}

impl EmbeddingModel {
    pub fn new(model_path: &str) -> Result<Self> {
        // Load ONNX model
        let session = Session::builder()?
            .with_model_from_file(model_path)?;
        
        // bge-m3 has 1024 dimensions
        Ok(Self {
            session,
            dimension: 1024,
        })
    }
    
    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        // Preprocess text (tokenization, etc.)
        let tokens = self.tokenize(text)?;
        
        // Run inference
        let input = Value::from_array(
            self.session.allocator(),
            &[1, tokens.len() as i64]
        )?;
        
        let outputs = self.session.run(vec![input])?;
        let embedding: Vec<f32> = outputs[0]
            .try_extract::<f32>()?
            .view()
            .to_slice()
            .to_vec();
        
        Ok(embedding)
    }
    
    pub fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        texts.iter().map(|text| self.embed(text)).collect()
    }
    
    fn tokenize(&self, text: &str) -> Result<Vec<i64>> {
        // Simple tokenization (replace with proper tokenizer)
        // For bge-m3, you'd use sentence-transformers tokenizer
        // This is a placeholder
        Ok(text.split_whitespace()
            .take(512)  // Max sequence length
            .map(|_| 1i64)
            .collect())
    }
    
    pub fn dimension(&self) -> usize {
        self.dimension
    }
}

// Alternative: Use Python subprocess for sentence-transformers
// This is simpler but requires Python
pub struct PythonEmbeddingModel {
    model_name: String,
}

impl PythonEmbeddingModel {
    pub fn new(model_name: &str) -> Self {
        Self {
            model_name: model_name.to_string(),
        }
    }
    
    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        use std::process::Command;
        
        let script = format!(
            r#"
import sys
from sentence_transformers import SentenceTransformer
import json

model = SentenceTransformer('{}')
embedding = model.encode('{}')
print(json.dumps(embedding.tolist()))
"#,
            self.model_name,
            text.replace("'", "\\'")
        );
        
        let output = Command::new("python3")
            .arg("-c")
            .arg(&script)
            .output()?;
        
        let json_str = String::from_utf8(output.stdout)?;
        let embedding: Vec<f32> = serde_json::from_str(&json_str)?;
        Ok(embedding)
    }
}
```

### Step 6: Database Schema Extension

```rust
// Add to src/storage/sqlite_storage_simple.rs

impl SqliteStorage {
    pub fn init_knowledge_base_tables(&self) -> SqliteResult<()> {
        // Documents table
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS documents (
                id TEXT PRIMARY KEY,
                file_path TEXT NOT NULL UNIQUE,
                file_name TEXT NOT NULL,
                file_type TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                metadata TEXT
            )",
            [],
        )?;
        
        // Document chunks table
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS document_chunks (
                id TEXT PRIMARY KEY,
                document_id TEXT NOT NULL,
                chunk_index INTEGER NOT NULL,
                content TEXT NOT NULL,
                embedding BLOB NOT NULL,
                FOREIGN KEY (document_id) REFERENCES documents(id) ON DELETE CASCADE
            )",
            [],
        )?;
        
        // Indexes
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_chunks_document ON document_chunks(document_id)",
            [],
        )?;
        
        // FTS5 for content search
        self.conn.execute(
            "CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(
                content,
                content = 'document_chunks',
                content_rowid = 'id'
            )",
            [],
        )?;
        
        Ok(())
    }
    
    pub fn insert_document(
        &self,
        file_path: &str,
        file_name: &str,
        file_type: &str,
        metadata: Option<&str>,
    ) -> SqliteResult<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let created_at = chrono::Utc::now().timestamp();
        
        self.conn.execute(
            "INSERT INTO documents (id, file_path, file_name, file_type, created_at, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, file_path, file_name, file_type, created_at, metadata],
        )?;
        
        Ok(id)
    }
    
    pub fn insert_chunk(
        &self,
        document_id: &str,
        chunk_index: usize,
        content: &str,
        embedding: &[f32],
    ) -> SqliteResult<String> {
        let id = uuid::Uuid::new_v4().to_string();
        
        // Convert embedding to bytes
        let embedding_bytes: Vec<u8> = embedding
            .iter()
            .flat_map(|&f| f.to_le_bytes())
            .collect();
        
        self.conn.execute(
            "INSERT INTO document_chunks (id, document_id, chunk_index, content, embedding)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, document_id, chunk_index, content, embedding_bytes],
        )?;
        
        // Update FTS5
        self.conn.execute(
            "INSERT INTO chunks_fts(rowid, content) VALUES ((SELECT rowid FROM document_chunks WHERE id = ?1), ?2)",
            params![id, content],
        )?;
        
        Ok(id)
    }
    
    pub fn search_similar_chunks(
        &self,
        query_embedding: &[f32],
        limit: usize,
    ) -> SqliteResult<Vec<SimilarChunk>> {
        // Load all chunks with embeddings
        let mut stmt = self.conn.prepare(
            "SELECT id, document_id, chunk_index, content, embedding FROM document_chunks"
        )?;
        
        let mut candidates: Vec<(String, String, usize, String, Vec<f32>, f32)> = Vec::new();
        
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, usize>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Vec<u8>>(4)?,
            ))
        })?;
        
        for row in rows {
            let (id, doc_id, chunk_idx, content, emb_bytes) = row?;
            
            // Convert bytes back to f32
            let chunk_embedding: Vec<f32> = emb_bytes
                .chunks_exact(4)
                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect();
            
            // Calculate cosine similarity
            let similarity = cosine_similarity(query_embedding, &chunk_embedding);
            
            candidates.push((id, doc_id, chunk_idx, content, chunk_embedding, similarity));
        }
        
        // Sort by similarity and take top-k
        candidates.sort_by(|a, b| b.5.partial_cmp(&a.5).unwrap());
        candidates.truncate(limit);
        
        Ok(candidates.into_iter().map(|(id, doc_id, chunk_idx, content, _, sim)| {
            SimilarChunk {
                id,
                document_id: doc_id,
                chunk_index: chunk_idx,
                content,
                similarity: sim,
            }
        }).collect())
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    
    dot_product / (norm_a * norm_b)
}

pub struct SimilarChunk {
    pub id: String,
    pub document_id: String,
    pub chunk_index: usize,
    pub content: String,
    pub similarity: f32,
}
```

### Step 7: RAG Retrieval Integration

```rust
// src/storage/rag_retrieval.rs
use crate::storage::{embeddings::EmbeddingModel, sqlite_storage_simple::SqliteStorage};
use anyhow::Result;

pub struct RAGRetriever {
    storage: SqliteStorage,
    embedding_model: EmbeddingModel,
}

impl RAGRetriever {
    pub fn new(storage: SqliteStorage, embedding_model: EmbeddingModel) -> Self {
        Self {
            storage,
            embedding_model,
        }
    }
    
    pub fn retrieve_context(&self, query: &str, top_k: usize) -> Result<Vec<String>> {
        // Generate query embedding
        let query_embedding = self.embedding_model.embed(query)?;
        
        // Search similar chunks
        let chunks = self.storage.search_similar_chunks(&query_embedding, top_k)?;
        
        // Return content
        Ok(chunks.into_iter().map(|c| c.content).collect())
    }
    
    pub fn add_document(&self, file_path: &str) -> Result<String> {
        use crate::storage::document_parser::parse_document;
        use crate::storage::chunking::chunk_text;
        use std::path::Path;
        
        // Parse document
        let parsed = parse_document(Path::new(file_path))?;
        
        // Chunk text
        let chunks = chunk_text(&parsed.content, &Default::default());
        
        // Insert document
        let doc_id = self.storage.insert_document(
            file_path,
            &parsed.metadata.file_name,
            &format!("{:?}", parsed.metadata.file_type),
            None,
        )?;
        
        // Generate embeddings and insert chunks
        for (idx, chunk) in chunks.iter().enumerate() {
            let embedding = self.embedding_model.embed(chunk)?;
            self.storage.insert_chunk(&doc_id, idx, chunk, &embedding)?;
        }
        
        Ok(doc_id)
    }
}
```

### Step 8: Integration with LLM Loop

```rust
// In src/agentic/loop_engine.rs or similar

// Before sending to LLM, check if RAG should be used
if should_use_rag(&user_message) {
    let rag_context = rag_retriever.retrieve_context(&user_message, 3)?;
    
    // Prepend context to system prompt or user message
    let enhanced_prompt = format!(
        "Context from knowledge base:\n{}\n\nUser question: {}",
        rag_context.join("\n\n"),
        user_message
    );
    
    // Use enhanced_prompt instead of user_message
}
```

## 🚀 Getting Started

1. **Download bge-m3 model**:
   ```bash
   # Option 1: Use Python to download and convert to ONNX
   python3 -c "from sentence_transformers import SentenceTransformer; \
               model = SentenceTransformer('BAAI/bge-m3'); \
               model.save('models/bge-m3')"
   
   # Option 2: Use ONNX model directly if available
   ```

2. **Test document parsing**:
   ```rust
   let parsed = parse_document(Path::new("test.pdf"))?;
   println!("Parsed {} words", parsed.metadata.word_count);
   ```

3. **Test embedding**:
   ```rust
   let model = EmbeddingModel::new("models/bge-m3.onnx")?;
   let embedding = model.embed("Hello world")?;
   println!("Embedding dimension: {}", embedding.len());
   ```

4. **Add document to knowledge base**:
   ```rust
   let rag = RAGRetriever::new(storage, model);
   let doc_id = rag.add_document("path/to/document.pdf")?;
   ```

5. **Query knowledge base**:
   ```rust
   let context = rag.retrieve_context("What is the main topic?", 3)?;
   ```

## 📝 Notes

- **Model Size**: bge-m3 is ~1.5GB. Consider smaller models for initial testing.
- **Performance**: ONNX runtime is faster than Python subprocess but requires model conversion.
- **Alternative Models**: Consider `all-MiniLM-L6-v2` (smaller, faster) or `multilingual-e5-base` (multilingual).
- **Vector Search**: For large-scale (>10k documents), consider external vector DB like Qdrant.








