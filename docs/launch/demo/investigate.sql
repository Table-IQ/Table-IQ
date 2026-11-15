-- Which paid orders have no shipped shipment?
SELECT o.id, c.name AS customer, o.total_cents
FROM orders AS o
JOIN customers AS c ON c.id = o.customer_id
WHERE EXISTS (
  SELECT 1 FROM payments AS p
  WHERE p.order_id = o.id AND p.status = 'paid'
)
AND NOT EXISTS (
  SELECT 1 FROM shipments AS s
  WHERE s.order_id = o.id AND s.status = 'shipped'
)
ORDER BY o.id;
