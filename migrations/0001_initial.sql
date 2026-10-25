-- A store is one permanent mapping: subdomain = source (a Google Sheet). Everything the
-- merchant edits lives in the source; this database keeps only what checkout needs.
CREATE TABLE stores (
  id TEXT PRIMARY KEY,
  slug TEXT NOT NULL UNIQUE,
  source_kind TEXT NOT NULL,
  source_id TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','inactive')),
  metadata_json TEXT NOT NULL,
  revision INTEGER NOT NULL DEFAULT 0 CHECK(revision>=0),
  -- Where Stor writes in the source: the Payments status cell and the History tab.
  status_tab INTEGER,
  status_row INTEGER,
  history_tab INTEGER,
  -- The last status written to the source, so an unchanged one is not written again.
  status_text TEXT,
  -- Since when the source can't be read; the store is deactivated after a while.
  failing_since INTEGER,
  next_sync INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL
);
CREATE UNIQUE INDEX one_store_per_source ON stores(source_kind, source_id) WHERE status='active';
CREATE INDEX store_sync ON stores(next_sync) WHERE status='active';
CREATE TRIGGER immutable_store BEFORE UPDATE OF id,slug,source_kind,source_id ON stores
BEGIN SELECT RAISE(ABORT,'immutable_store'); END;

CREATE TABLE products (
  store_id TEXT NOT NULL REFERENCES stores(id),
  sku TEXT NOT NULL,
  title TEXT NOT NULL,
  description TEXT NOT NULL,
  -- The first price as the merchant wrote it, for display: `$25`, `25 USDC`.
  price TEXT NOT NULL DEFAULT '',
  payments_json TEXT NOT NULL,
  images_json TEXT NOT NULL DEFAULT '[]',
  capacity INTEGER NOT NULL CHECK(capacity>=0),
  reserved INTEGER NOT NULL DEFAULT 0 CHECK(reserved>=0),
  sold INTEGER NOT NULL DEFAULT 0 CHECK(sold>=0),
  active INTEGER NOT NULL DEFAULT 1 CHECK(active IN (0,1)),
  delivery_ciphertext TEXT NOT NULL,
  source_revision INTEGER NOT NULL,
  PRIMARY KEY(store_id,sku),
  CHECK(capacity>=reserved+sold)
);

CREATE TABLE customers (
  store_id TEXT NOT NULL REFERENCES stores(id),
  id_hash BLOB NOT NULL,
  created_at INTEGER NOT NULL,
  PRIMARY KEY(store_id,id_hash)
);

CREATE TABLE carts (
  store_id TEXT NOT NULL,
  customer_hash BLOB NOT NULL,
  sku TEXT NOT NULL,
  quantity INTEGER NOT NULL CHECK(quantity BETWEEN 1 AND 100),
  PRIMARY KEY(store_id,customer_hash,sku),
  FOREIGN KEY(store_id,customer_hash) REFERENCES customers(store_id,id_hash),
  FOREIGN KEY(store_id,sku) REFERENCES products(store_id,sku)
);

-- The payment server and key from the source's Payments tab. A changed URL or KEY is a
-- new connection; a retired one keeps its webhook secret for its orders' late events.
CREATE TABLE connections (
  store_id TEXT NOT NULL REFERENCES stores(id),
  id TEXT NOT NULL,
  origin TEXT NOT NULL,
  project TEXT NOT NULL,
  key_hash BLOB NOT NULL,
  access_ciphertext TEXT NOT NULL,
  webhook_id TEXT NOT NULL,
  webhook_ciphertext TEXT NOT NULL,
  hook_id TEXT NOT NULL UNIQUE,
  status TEXT NOT NULL CHECK(status IN ('active','retired')),
  view_json TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  PRIMARY KEY(store_id,id)
);
CREATE UNIQUE INDEX one_active_connection ON connections(store_id) WHERE status='active';

CREATE TABLE orders (
  store_id TEXT NOT NULL,
  id TEXT NOT NULL,
  customer_hash BLOB NOT NULL,
  connection_id TEXT NOT NULL,
  reference TEXT NOT NULL,
  status TEXT NOT NULL CHECK(status IN ('creating','awaiting_payment','partial','seen','delivered','expired','late_payment','manual_review','failed')),
  chain TEXT NOT NULL,
  asset TEXT NOT NULL,
  expected_amount TEXT NOT NULL,
  address TEXT,
  items_json TEXT NOT NULL,
  delivery_ciphertext TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  expires_at INTEGER NOT NULL,
  transaction_id TEXT,
  PRIMARY KEY(store_id,id),
  UNIQUE(store_id,connection_id,reference),
  FOREIGN KEY(store_id,customer_hash) REFERENCES customers(store_id,id_hash),
  FOREIGN KEY(store_id,connection_id) REFERENCES connections(store_id,id)
);
CREATE TRIGGER immutable_order BEFORE UPDATE OF store_id,id,customer_hash,connection_id,reference,chain,asset,expected_amount,items_json,delivery_ciphertext ON orders
BEGIN SELECT RAISE(ABORT,'immutable_order'); END;
CREATE TRIGGER immutable_address BEFORE UPDATE OF address ON orders
WHEN OLD.address IS NOT NULL AND NEW.address IS NOT OLD.address
BEGIN SELECT RAISE(ABORT,'immutable_address'); END;

CREATE TABLE payments (
  store_id TEXT NOT NULL,
  order_id TEXT NOT NULL,
  transfer TEXT NOT NULL,
  amount TEXT NOT NULL,
  confirmed INTEGER NOT NULL CHECK(confirmed IN (0,1)),
  PRIMARY KEY(store_id,order_id,transfer),
  FOREIGN KEY(store_id,order_id) REFERENCES orders(store_id,id)
);

CREATE TABLE allocations (
  store_id TEXT NOT NULL,
  order_id TEXT NOT NULL,
  sku TEXT NOT NULL,
  quantity INTEGER NOT NULL CHECK(quantity>0),
  state TEXT NOT NULL CHECK(state IN ('reserved','sold','released')),
  PRIMARY KEY(store_id,order_id,sku),
  FOREIGN KEY(store_id,order_id) REFERENCES orders(store_id,id),
  FOREIGN KEY(store_id,sku) REFERENCES products(store_id,sku)
);

CREATE TABLE received_events (
  store_id TEXT NOT NULL,
  connection_id TEXT NOT NULL,
  id TEXT NOT NULL,
  body_hash BLOB NOT NULL,
  received_at INTEGER NOT NULL,
  PRIMARY KEY(store_id,connection_id,id),
  FOREIGN KEY(store_id,connection_id) REFERENCES connections(store_id,id)
);

-- Paid orders waiting to be appended to the source's History tab.
CREATE TABLE history_outbox (
  store_id TEXT NOT NULL,
  order_id TEXT NOT NULL,
  next_attempt INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY(store_id,order_id),
  FOREIGN KEY(store_id,order_id) REFERENCES orders(store_id,id)
);
CREATE TRIGGER history_order_paid AFTER UPDATE OF status ON orders
WHEN NEW.status IN ('delivered','late_payment') AND OLD.status NOT IN ('delivered','late_payment')
BEGIN
  INSERT INTO history_outbox(store_id,order_id) VALUES(NEW.store_id,NEW.id) ON CONFLICT DO NOTHING;
END;

CREATE TABLE altcha_replay (fingerprint BLOB PRIMARY KEY, purpose TEXT NOT NULL, expires_at INTEGER NOT NULL);
CREATE TABLE rate_limits (identity BLOB NOT NULL, bucket TEXT NOT NULL, window INTEGER NOT NULL, count INTEGER NOT NULL, expires_at INTEGER NOT NULL, PRIMARY KEY(identity,bucket,window));
CREATE INDEX rate_limit_expiry ON rate_limits(expires_at);
