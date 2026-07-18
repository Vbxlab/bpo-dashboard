#!/usr/bin/env bash
set -e

# -------------------------------------------------------------------
# run_dashboard.sh – Installe, configure et lance le BPO Dashboard.
# -------------------------------------------------------------------
# 1. Crée le répertoire data/ s'il n'existe pas.
# 2. Copie config.example.json → data/config.json si le fichier n'existe pas.
# 3. Demande interactivement le client_id et le client_secret et les écrit
#    dans le fichier config.json.
# 4. Compile (release) si le binaire n'est pas présent.
# 5. Lance le serveur et ouvre le navigateur.
# -------------------------------------------------------------------

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DATA_DIR="${REPO_DIR}/data"
EXAMPLE_CFG="${REPO_DIR}/config.example.json"
REAL_CFG="${DATA_DIR}/config.json"
EXECUTABLE="${REPO_DIR}/target/release/bpo-dashboard"
URL="http://localhost:8090"

# ----- 1️⃣ Création du répertoire data/ ---------------------------
if [[ ! -d "${DATA_DIR}" ]]; then
    echo "📁  Création du répertoire data → ${DATA_DIR}"
    mkdir -p "${DATA_DIR}"
fi

# ----- 2️⃣ Copie du modèle de config si nécessaire ---------------
if [[ ! -f "${REAL_CFG}" ]]; then
    if [[ -f "${EXAMPLE_CFG}" ]]; then
        echo "📄  Copie du fichier modèle → ${REAL_CFG}"
        cp "${EXAMPLE_CFG}" "${REAL_CFG}"
    else
        echo "❌  Fichier modèle introuvable : ${EXAMPLE_CFG}"
        exit 1
    fi
fi

# ----- 3️⃣ Demande des identifiants EVE --------------------------
echo "🔐  Veuillez fournir les credentials de votre application EVE."
read -p "Client ID : " client_id
# -s pour ne pas afficher le secret à l'écran
read -s -p "Client Secret : " client_secret
echo   # saut de ligne après le secret

# Met à jour le JSON en remplaçant les placeholders
# On utilise sed – il faut être prudent avec les / dans les valeurs.
# On échappe les / dans les variables.
escaped_id=$(printf '%s' "$client_id" | sed 's/[\/&]/\\&/g')
escaped_secret=$(printf '%s' "$client_secret" | sed 's/[\/&]/\\&/g')

# Remplace les champs dans le fichier config.json
sed -i "s/\"client_id\": \"[^\"]*\"/\"client_id\": \"${escaped_id}\"/" "${REAL_CFG}"
sed -i "s/\"client_secret\": \"[^\"]*\"/\"client_secret\": \"${escaped_secret}\"/" "${REAL_CFG}"

echo "✅  Credentials enregistrés dans ${REAL_CFG}."

# ----- 4️⃣ Compilation si le binaire n'existe pas ----------------
if [[ ! -x "${EXECUTABLE}" ]]; then
    echo "🔨  Compilation du projet en mode release…"
    cd "${REPO_DIR}"
    cargo build --release
fi

# ----- 5️⃣ Lancement du serveur -----------------------------------
echo "🚀  Démarrage du dashboard…"
"${EXECUTABLE}" &
PID=$!
# Petite pause pour laisser le serveur s'initialiser
sleep 2

# ----- 6️⃣ Ouverture du navigateur -------------------------------
if command -v xdg-open >/dev/null 2>&1; then
    xdg-open "${URL}" &>/dev/null || true
elif command -v open >/dev/null 2>&1; then
    open "${URL}" &>/dev/null || true
else
    echo "🔗  Ouvre manuellement ton navigateur à l'adresse : ${URL}"
fi

# Attente du serveur (CTRL‑C pour stopper)
wait $PID
