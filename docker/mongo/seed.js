// The healthcheck, in ONE mongosh call (env PORT = this server's port): initiate the replica
// set if this is an uninitiated member, require a writable primary, then seed `tableiq` once.
// "Once" is the `_tiq_meta` marker, written last: a later edit, rename or delete of seed data
// never triggers a re-seed. Tests treat users/orders as read-only (they write to collections
// of their own).
const h0 = db.hello();
if (h0.isreplicaset && !h0.setName) {
  rs.initiate({ _id: "rs0", members: [{ _id: 0, host: "127.0.0.1:" + process.env.PORT }] });
}
if (!db.hello().isWritablePrimary) quit(1);

const d = db.getSiblingDB("tableiq");
const meta = d.getCollection("_tiq_meta");
if (!meta.findOne({ _id: "seeded" })) {
  d.users.insertMany([
    { name: "Ada", age: 36, email: "ada@example.com", tags: ["math", "engines"], address: { city: "London", zip: "N1" } },
    { name: "Linus", age: 54, email: "linus@example.com", tags: ["kernel"], address: { city: "Portland", zip: "97201" } },
    { name: "Grace", age: 85, email: "grace@example.com", tags: ["compilers", "navy"], address: { city: "Arlington", zip: "22201" } },
  ]);
  d.orders.insertMany([
    { customer: "Ada", total: 99.5, items: [{ sku: "A-1", qty: 2 }, { sku: "B-7", qty: 1 }], shipping: { method: "air", address: { city: "London" } } },
    { _id: "ord-string-id", customer: "Linus", total: 12, items: [], shipping: { method: "sea", address: { city: "Portland" } } },
    { _id: 42, customer: "Grace", total: 7.25, items: [{ sku: "C-3", qty: 10 }], shipping: null },
  ]);
  meta.insertOne({ _id: "seeded" });
}
