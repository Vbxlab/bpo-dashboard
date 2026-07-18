#!/usr/bin/env bash
set -e

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DATA_DIR="${REPO_DIR}/data"
EXAMPLE_CFG="${REPO_DIR}/config.example.json"
REAL_CFG="${DATA_DIR}/config.json"
EXECUTABLE="${REPO_DIR}/target/release/bpo-dashboard"
URL="http://localhost:8090"

# 1️⃣ Crée data/ si manquant
if [[ ! -d "${DATA_DIR}" ]]; then
    mkdir -p "${DATA_DIR}"
fi

# 2️⃣ Copie le template si manquant
if [[ ! -f "${REAL_CFG}" ]]; then
    cp "${EXAMPLE_CFG}" "${REAL_CFG}"
fi

# 3️⃣ Demande les credentials (affichés à l'écran, pas masqués)
read -p "Client ID : " client_id
read -p "Client Secret : " client_secret

escaped_id=$(printf '%s' "$client_id" | sed 's/[\/&]/\\&/g')
escaped_secret=$(printf '%s' "$client_secret" | sed 's/[\/&]/\\&/g')

sed -i "s/\"client_id\": \"[^\"]*\"/\"client_id\": \"${escaped_id}\"/" "${REAL_CFG}"
sed -i "s/\"client_secret\": \"[^\"]*\"/\"client_secret\": \"${escaped_secret}\"/" "${REAL_CFG}"

# 4️⃣ Confirmation
echo "✅  client_id     : ${client_id}"
echo "✅  client_secret : ${client_secret}"

# 5️⃣ Compile si besoin
if [[ ! -x "${EXECUTABLE}" ]]; then
    cd "${REPO_DIR}"
    cargo build --release
fi

# 6️⃣ Lance
echo "🚀  Dashboard sur ${URL}"
"${EXECUTABLE}" &
PID=$!
sleep 2
xdg-open "${URL}" >/dev/null 2>&1 || true
wait $PID