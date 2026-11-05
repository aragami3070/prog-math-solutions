# Lemma table

`build.py` tags the whole corpus with spaCy (`en_core_web_sm`, parser and NER off) and writes
`data/lemmas.json`: surface form -> lemma (+ part of speech), the lemma each form most often
got in context. The search reads it at start-up (`LEMMA_FILE`, default `data/lemmas.json`).

Set up (no system pip needed):

    python3 -m venv --without-pip .venv-nlp
    curl -sS https://bootstrap.pypa.io/get-pip.py | .venv-nlp/bin/python
    .venv-nlp/bin/pip install spacy==3.8.2 click \
      https://github.com/explosion/spacy-models/releases/download/en_core_web_sm-3.8.0/en_core_web_sm-3.8.0-py3-none-any.whl
    .venv-nlp/bin/python tools/lemmas/build.py data/lemmas.json

Needs the database container running (`docker-compose up -d postgres`). About 20 minutes for
800 sampled documents on one CPU. English only: Russian queries are not lemmatized.
