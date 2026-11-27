#!/usr/bin/env python3
"""Validate untrusted model classifications against supplied document sections."""
import hashlib
import json
from pathlib import Path

TAXONOMY = Path(__file__).with_name("topics-v1.json")


def load_taxonomy():
    data = json.loads(TAXONOMY.read_text(encoding="utf-8"))
    ids = [topic["id"] for topic in data["topics"]]
    if len(ids) != len(set(ids)):
        raise ValueError("duplicate topic ID")
    return data


def input_hash(title, sections):
    """Hash the complete input, not just the first page or a stored page hash."""
    data = json.dumps({"title": title, "sections": sections}, ensure_ascii=False,
                      sort_keys=True, separators=(",", ":"))
    return hashlib.sha256(data.encode("utf-8")).hexdigest()


def validate(result, sections, taxonomy=None):
    """Validate a chunk result; evidence must quote its exact supplied section text."""
    taxonomy = taxonomy or load_taxonomy()
    allowed = {topic["id"] for topic in taxonomy["topics"]}
    if not isinstance(result, dict) or set(result) != {"topics"}:
        raise ValueError("expected an object containing only topics")
    topics = result["topics"]
    if not isinstance(topics, list) or len(topics) > 5:
        raise ValueError("topics must be a list of at most five items")
    ids = set()
    for topic in topics:
        if not isinstance(topic, dict) or set(topic) != {"topic_id", "evidence"}:
            raise ValueError("invalid topic object")
        topic_id = topic["topic_id"]
        if not isinstance(topic_id, str) or topic_id not in allowed or topic_id in ids:
            raise ValueError("unknown or duplicate topic ID")
        ids.add(topic_id)
        evidence = topic["evidence"]
        if not isinstance(evidence, list) or not evidence or len(evidence) > 3:
            raise ValueError("each topic needs one to three evidence quotes")
        for item in evidence:
            if not isinstance(item, dict) or set(item) != {"section_id", "quote"}:
                raise ValueError("invalid evidence object")
            section_id, quote = item["section_id"], item["quote"]
            if (not isinstance(section_id, str) or section_id not in sections
                    or not isinstance(quote, str) or not quote.strip()
                    or len(quote) > 500 or quote not in sections[section_id]):
                raise ValueError("evidence quote not present in section")
    return result
