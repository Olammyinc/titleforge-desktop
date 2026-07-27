// Minimal HTTP server for Tauri dev mode — serves ../src
var http = require('http'), fs = require('fs'), path = require('path');
var srcDir = path.join(__dirname, 'src');
var mime = { '.html': 'text/html', '.css': 'text/css', '.js': 'application/javascript', '.svg': 'image/svg+xml', '.json': 'application/json', '.png': 'image/png' };

http.createServer(function (req, res) {
  var filePath = path.join(srcDir, req.url.split('?')[0]);
  if (filePath.endsWith('/')) filePath = path.join(filePath, 'index.html');
  fs.readFile(filePath, function (err, data) {
    if (err) { res.writeHead(404); res.end('Not Found'); return; }
    var ext = path.extname(filePath).toLowerCase();
    res.writeHead(200, { 'Content-Type': mime[ext] || 'application/octet-stream' });
    res.end(data);
  });
}).listen(1420, function () { console.log('Dev server running on http://localhost:1420'); });
