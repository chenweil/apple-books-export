#!/bin/bash
# 安装 Apple Books Exporter skill

set -e

# 检测当前平台
detect_platform() {
    OS=$(uname -s)
    ARCH=$(uname -m)
    
    case "$OS" in
        Darwin)
            if [ "$ARCH" = "arm64" ]; then
                echo "aarch64-apple-darwin"
            else
                echo "x86_64-apple-darwin"
            fi
            ;;
        Linux)
            echo "x86_64-unknown-linux-gnu"
            ;;
        *)
            echo "unsupported"
            ;;
    esac
}

# 安装目录
SKILL_DIR="$HOME/.agents/skills/apple-books-export-rust"
SCRIPTS_DIR="$SKILL_DIR/scripts"

echo "📦 Apple Books Exporter Skill 安装脚本"
echo "======================================"

# 检测平台
PLATFORM=$(detect_platform)
if [ "$PLATFORM" = "unsupported" ]; then
    echo "❌ 不支持的操作系统"
    exit 1
fi

echo "🔍 检测到平台: $PLATFORM"

# 创建目录
echo "📁 创建目录..."
mkdir -p "$SCRIPTS_DIR"

# 复制 SKILL.md
echo "📄 复制 SKILL.md..."
cp "$(dirname "$0")/../SKILL.md" "$SKILL_DIR/"

# 查找二进制文件
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BINARY_NAME="apple-books-exporter"

# 优先使用平台特定版本，否则使用通用版本
if [ -f "$SCRIPT_DIR/${BINARY_NAME}-${PLATFORM}" ]; then
    BINARY="$SCRIPT_DIR/${BINARY_NAME}-${PLATFORM}"
elif [ -f "$SCRIPT_DIR/${BINARY_NAME}" ]; then
    BINARY="$SCRIPT_DIR/${BINARY_NAME}"
else
    echo "❌ 找不到二进制文件: ${BINARY_NAME} 或 ${BINARY_NAME}-${PLATFORM}"
    echo "   请先运行 build.sh 编译"
    exit 1
fi

# 复制二进制
echo "📦 安装二进制: $(basename $BINARY)"
cp "$BINARY" "$SCRIPTS_DIR/apple-books-exporter"
chmod +x "$SCRIPTS_DIR/apple-books-exporter"

# 验证安装
echo ""
echo "✅ 安装完成！"
echo ""
echo "使用方法:"
echo "  ~/.agents/skills/apple-books-export-rust/scripts/apple-books-exporter list"
echo ""
echo "或添加到 PATH:"
echo "  cp ~/.agents/skills/apple-books-export-rust/scripts/apple-books-exporter /usr/local/bin/"
echo ""
echo "⚠️  注意: 需要在系统设置中授予终端 Full Disk Access 权限"
echo "   System Settings → Privacy & Security → Full Disk Access → Add Terminal"
