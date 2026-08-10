#!/usr/bin/env bash
# 安装 Apple Books Exporter skill

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPOSITORY_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

if [ "$(uname -s)" != "Darwin" ]; then
    echo "❌ Apple Books Exporter Skill 仅支持 macOS"
    exit 1
fi

# 检测当前平台
detect_platform() {
    ARCH=$(uname -m)
    
    case "$ARCH" in
        arm64|aarch64)
            echo "aarch64-apple-darwin"
            ;;
        x86_64|amd64)
            echo "x86_64-apple-darwin"
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
cp "$SCRIPT_DIR/../SKILL.md" "$SKILL_DIR/"
cp "$SCRIPT_DIR/validate.sh" "$SCRIPTS_DIR/validate.sh"
chmod +x "$SCRIPTS_DIR/validate.sh"

# 查找二进制文件
BINARY_NAME="apple-books-exporter"

# 优先使用仓库 release binary；否则使用平台特定版本或通用版本。
if [ -x "$REPOSITORY_ROOT/target/release/$BINARY_NAME" ]; then
    BINARY="$REPOSITORY_ROOT/target/release/$BINARY_NAME"
elif [ -f "$SCRIPT_DIR/${BINARY_NAME}-${PLATFORM}" ]; then
    BINARY="$SCRIPT_DIR/${BINARY_NAME}-${PLATFORM}"
elif [ -f "$SCRIPT_DIR/${BINARY_NAME}" ]; then
    BINARY="$SCRIPT_DIR/${BINARY_NAME}"
else
    echo "❌ 找不到二进制文件: ${BINARY_NAME} 或 ${BINARY_NAME}-${PLATFORM}"
    echo "   请先运行 build.sh 编译"
    exit 1
fi

# 在安装前拒绝旧协议、错误架构或不可执行的 binary。
echo "🔎 验证 binary..."
APPLE_BOOKS_EXPORTER_BIN="$BINARY" "$SCRIPT_DIR/validate.sh" --print-path >/dev/null

# 复制二进制
echo "📦 安装二进制: $(basename "$BINARY")"
cp "$BINARY" "$SCRIPTS_DIR/apple-books-exporter"
chmod +x "$SCRIPTS_DIR/apple-books-exporter"

# 再验证安装后的实际路径，避免复制过程产生不可用文件。
APPLE_BOOKS_EXPORTER_BIN="$SCRIPTS_DIR/apple-books-exporter" \
    "$SCRIPTS_DIR/validate.sh" --print-path >/dev/null

# 验证安装
echo ""
echo "✅ 安装完成！"
echo ""
echo "使用方法:"
echo "  ~/.agents/skills/apple-books-export-rust/scripts/validate.sh --print-path"
echo "  ~/.agents/skills/apple-books-export-rust/scripts/apple-books-exporter list --json"
echo ""
echo "或添加到 PATH:"
echo "  cp ~/.agents/skills/apple-books-export-rust/scripts/apple-books-exporter /usr/local/bin/"
echo ""
echo "⚠️  注意: 需要在系统设置中授予终端 Full Disk Access 权限"
echo "   System Settings → Privacy & Security → Full Disk Access → Add Terminal"
