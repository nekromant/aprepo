#!/usr/bin/env python3
"""Simple HTTP server for WebDL integration tests."""
import http.server
import socketserver
import sys
import os
import time

PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 8765
DURATION = int(sys.argv[2]) if len(sys.argv) > 2 else 5
DIRECTORY = os.path.dirname(os.path.abspath(__file__))

class Handler(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=DIRECTORY, **kwargs)

    def log_message(self, format, *args):
        pass

class ReuseAddrServer(socketserver.TCPServer):
    allow_reuse_address = True

with ReuseAddrServer(("", PORT), Handler) as httpd:
    print(f"Serving {DIRECTORY} on port {PORT} for {DURATION}s")
    httpd.timeout = DURATION
    start = time.time()
    while time.time() - start < DURATION:
        httpd.handle_request()
    print("Server shutting down")
