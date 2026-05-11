#!/bin/bash

echo "Building books-exporter binary..."

pyinstaller --onefile --name books-exporter --clean books_exporter.py

if [ -f dist/books-exporter ]; then
    echo "✅ Build successful: dist/books-exporter"
    ls -lh dist/books-exporter
    echo ""
    echo "Test the binary:"
    echo "  ./dist/books-exporter list"
    echo "  ./dist/books-exporter export -t '纳瓦尔' -o ~/Desktop"
else
    echo "❌ Build failed"
    exit 1
fi