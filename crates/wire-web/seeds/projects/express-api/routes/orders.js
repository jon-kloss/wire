const express = require("express");
const router = express.Router();

const orders = [];

router.get("/orders", (req, res) => {
  res.json(orders);
});

router.get("/orders/:id", (req, res) => {
  res.json(orders.find((o) => o.id === Number(req.params.id)) || {});
});

router.post("/orders", (req, res) => {
  const order = { id: orders.length + 1, ...req.body };
  orders.push(order);
  res.status(201).json(order);
});

module.exports = router;
