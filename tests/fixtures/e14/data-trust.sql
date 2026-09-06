-- E14 deterministic beginner data-trust fixture.
-- Run in a disposable local Tarik project. The first section creates known failures.

CREATE OR REPLACE TABLE customers (
  customer_id BIGINT,
  customer_name VARCHAR
);

INSERT INTO customers VALUES
  (101, 'Amina Rahman'),
  (102, 'Mateo Silva'),
  (103, 'Lin Wei');

CREATE OR REPLACE TABLE orders (
  order_id BIGINT,
  customer_id BIGINT,
  amount DECIMAL(10, 2),
  status VARCHAR,
  ordered_at TIMESTAMP
);

INSERT INTO orders VALUES
  (7001, 101, 42.50, 'paid',      TIMESTAMP '2026-09-05 09:15:00'),
  (7002, NULL, 18.25, 'paid',     TIMESTAMP '2026-09-05 10:20:00'),
  (7002, 102, 1500.00, 'pending', TIMESTAMP '2026-09-04 14:05:00'),
  (7004, 999, -7.00, 'returned',  TIMESTAMP '2026-08-10 08:00:00'),
  (7005, 103, 63.10, 'mystery',   TIMESTAMP '2026-09-05 15:45:00');

-- Expected initial issues:
-- 1 NULL customer_id
-- 2 rows with duplicate order_id 7002
-- 2 amounts outside the inclusive range 0 through 1000
-- 1 unexpected status (mystery)
-- 1 unmatched customer_id (999)
-- Freshness outcome depends on current time; for deterministic automation use
-- a custom failure query with the fixed boundary 2026-09-01.

-- Explicit repair section. Run only after inspecting the failed checks.
UPDATE orders SET customer_id = 103 WHERE order_id = 7002 AND customer_id IS NULL;
UPDATE orders SET order_id = 7003 WHERE order_id = 7002 AND customer_id = 102;
UPDATE orders SET amount = 950.00 WHERE order_id = 7003;
UPDATE orders SET amount = 7.00, customer_id = 101, ordered_at = TIMESTAMP '2026-09-03 08:00:00'
WHERE order_id = 7004;
UPDATE orders SET status = 'pending' WHERE order_id = 7005;
