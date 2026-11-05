"""Shared bits of the embedding tools: the model, its prefixes, the Qdrant collection name."""
MODEL = "intfloat/multilingual-e5-small"
DIM = 384
CHUNKER_VERSION = "chunks-v1"
COLLECTION = "rks_chunks_v1"
# e5 models are trained with these prefixes; skipping them degrades retrieval.
PREFIX = {"query": "query: ", "passage": "passage: "}


def load_model():
    from sentence_transformers import SentenceTransformer
    model = SentenceTransformer(MODEL, device="cpu")
    model.max_seq_length = 320
    return model


def embed(model, texts, kind):
    vectors = model.encode([PREFIX[kind] + t for t in texts], batch_size=16,
                           normalize_embeddings=True, show_progress_bar=False)
    return vectors.tolist()
