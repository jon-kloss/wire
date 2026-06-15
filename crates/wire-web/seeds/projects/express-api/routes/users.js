const express = require("express");
const router = express.Router();

// In-memory user store for the sample API
const users = [];

router.get("/users", (req, res) => {
  res.json(users);
});

router.get("/users/:id", (req, res) => {
  const user = users.find((u) => u.id === Number(req.params.id));
  if (!user) return res.status(404).json({ error: "not found" });
  res.json(user);
});

router.post("/users", (req, res) => {
  const user = { id: users.length + 1, ...req.body };
  users.push(user);
  res.status(201).json(user);
});

router.put("/users/:id", (req, res) => {
  res.json({ id: Number(req.params.id), ...req.body });
});

router.delete("/users/:id", (req, res) => {
  res.status(204).end();
});

module.exports = router;
