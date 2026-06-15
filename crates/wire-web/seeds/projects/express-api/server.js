const express = require("express");
const app = express();

app.use(express.json());
app.use(require("./routes/users"));
app.use(require("./routes/orders"));

app.get("/health", (req, res) => {
  res.json({ status: "ok" });
});

app.listen(3000, () => console.log("listening on :3000"));
