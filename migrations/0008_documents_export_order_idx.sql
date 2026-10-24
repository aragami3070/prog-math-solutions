-- Export pages through documents with a keyset on (source, native_id) in C collation,
-- so each batch is an index range scan instead of a sort of the whole table.
CREATE INDEX documents_export_order_idx ON documents(source COLLATE "C", native_id COLLATE "C");
