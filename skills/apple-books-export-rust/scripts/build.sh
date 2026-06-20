#!/bin/bash
# 编译跨平台二进制文件

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

echo "🔨 Apple Books Exporter 跨平台编译"
echo "=================================="

cd "$PROJECT_ROOT"

# 编译函数
build_target() {
    TARGET=$1
    OUTPUT_NAME=$2
    
    echo ""
    echo "📦 编译 $TARGET..."
    
    # 检查是否安装了 target
    if ! rustup target list --installed | grep -q "$TARGET"; then
        echo "  安装 target: $TARGET"
        rustup target add "$TARGET"
    fi
    
    # 编译
    cargo build --release --target "$TARGET"
    
    # 复制并重命名
    BINARY="$PROJECT_ROOT/target/$TARGET/release/apple-books-exporter"
    if [ -f "$BINARY" ]; then
        cp "$BINARY" "$SCRIPT_DIR/$OUTPUT_NAME"
        chmod +x "$SCRIPT_DIR/$OUTPUT_NAME"
        SIZE=$(ls -lh "$SCRIPT_DIR/$OUTPUT_NAME" | awk '{print $5}')
        echo "  ✅ $OUTPUT_NAME ($SIZE)"
    else
        echo "  ❌ 编译失败: $BINARY not found"
        return 1
    fi
}

# 检测当前平台
OS=$(uname -s)
ARCH=$(uname -m)

echo "当前系统: $OS $ARCH"
echo ""

# macOS ARM (当前系统)
if [ "$OS" = "Darwin" ] && [ "$ARCH" = "arm64" ]; then
    echo "编译 macOS ARM (当前系统)..."
    build_target "aarch64-apple-darwin" "apple-books-exporter-aarch64-apple-darwin"
    
    # 也创建默认版本
    cp "$SCRIPT_DIR/apple-books-exporter-aarch64-apple-darwin" "$SCRIPT_DIR/apple-books-exporter"
    chmod +x "$SCRIPT_DIR/apple-books-exporter"
    echo "  ✅ apple-books-exporter (默认)"
fi

# macOS Intel (交叉编译需要额外配置)
# build_target "x86_64-apple-darwin" "apple-books-exporter-x86_64-apple-darwin"

# Linux (需要在 Linux 上编译或使用交叉编译工具链)
# build_target "x86_64-unknown-linux-gnu" "apple-books-exporter-x86_64-unknown-linux-gnu"

echo ""
echo "📊 编译结果:"
ls -lh "$SCRIPT_DIR"/apple-books-exporter* 2>/dev/null || echo "  无"

echo ""
echo "✅ 完成！"
echo ""
echo "安装 skill:"
echo "  cd $SCRIPT_DIR && ./install.sh"
