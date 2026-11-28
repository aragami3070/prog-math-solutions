"""Shared bits of the embedding tools: the model, its prefixes, the Qdrant collection name."""
MODEL = "intfloat/multilingual-e5-small"
DIM = 384
# v2 embeds every chunk of every document; v1 kept only the first four chunks of a document,
# which left the tail of two thirds of the corpus invisible to the vector side.
CHUNKER_VERSION = "chunks-v2"
COLLECTION = "rks_chunks_all_v2"
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
