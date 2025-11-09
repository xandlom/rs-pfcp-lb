#!/bin/bash
# Debug script for Docker compose issues

echo "=== Docker Compose Debug Script ==="
echo ""

echo "1. Checking container status..."
docker compose ps
echo ""

echo "2. Checking if binaries exist in the image..."
docker compose run --rm --entrypoint sh pfcp-proxy -c "ls -la /usr/local/bin/ | grep -E 'pfcp-proxy|test-upf|test-smf'"
echo ""

echo "3. Checking if binaries are executable..."
docker compose run --rm --entrypoint sh pfcp-proxy -c "file /usr/local/bin/pfcp-proxy /usr/local/bin/test-upf /usr/local/bin/test-smf"
echo ""

echo "4. Checking user permissions..."
docker compose run --rm --entrypoint sh pfcp-proxy -c "whoami && id"
echo ""

echo "5. Testing pfcp-proxy binary directly..."
docker compose run --rm --entrypoint /usr/local/bin/pfcp-proxy pfcp-proxy --help
echo ""

echo "6. Testing test-upf binary directly..."
docker compose run --rm --entrypoint /usr/local/bin/test-upf upf1 --help
echo ""

echo "7. Checking last logs from pfcp-proxy..."
docker compose logs --tail=50 pfcp-proxy
echo ""

echo "8. Checking last logs from upf1..."
docker compose logs --tail=50 upf1
echo ""

echo "9. Inspecting pfcp-proxy container..."
docker compose ps -a | grep pfcp-proxy
echo ""

echo "10. Checking for build errors..."
docker compose build pfcp-proxy 2>&1 | tail -20
echo ""
