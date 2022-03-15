const http = require("http");
const { chdir } = require("process");
const host = '0.0.0.0';
const port = 3000;
const server = http.createServer(onRequest);
server.listen(port, host, () => {
  console.log(`Running on http://${host}:${port}`);
});
function onRequest(req,res) {
  res.writeHead(200);
  res.end("Hello World!");
}