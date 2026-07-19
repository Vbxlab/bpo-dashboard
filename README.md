# BPO Dashboard

Dashboard web pour les BPO (Blueprint Originals) EVE Online.

- Recherche instantanée avec filtres (ME, TE, hub, profit)
- Résumé global (stats, top profits/pertes, investissement)
- Classement par hub avec tri par marge %
- BPOs à améliorer (ME/TE) avec économie estimée
- Bilan matériaux
- Multi-comptes EVE via SSO OAuth2
- Ajout/suppression de personnages via l'interface
- Interface en français
- Binaire unique portable (~6 MB)

## Installation

### Prérequis

- Rust 1.70+ (edition 2024) pour compiler

### Récupérer le code

```bash
git clone https://github.com/Vbxlab/bpo-dashboard.git
cd bpo-dashboard
```

## Configuration

### Étape 1 : Créer une application EVE Online

1. Va sur [developers.eveonline.com](https://developers.eveonline.com)
2. Connecte-toi avec ton compte EVE
3. Clique sur **Create New Application**
4. Remplis :
   - **Name** : `BPO Dashboard` (ou ce que tu veux)
   - **Description** : `Dashboard personnel pour suivre les BPO`
   - **Callback URL** : `http://localhost:8090/api/sso/callback` (**important !** c'est l'URL où EVE redirigera après connexion)
5. Dans **Enabled Scopes**, coche :
   - `esi-characters.read_blueprints.v1` (BPO + BPC personnels)
   - `esi-corporations.read_blueprints.v1` (BPO + BPC de corporation)
6. Sauvegarde

Tu obtiendras :
- **Client ID** : une chaîne comme `6a951f4d0e484cbeb8fd98d2815b5975`
- **Secret** : une chaîne comme `eat_1N...` (**garde-le secret !**)

### Étape 2 : Lancer le dashboard

```bash
./run_dashboard.sh
```

Le script compile le projet, configure les credentials au premier lancement et démarre le serveur.

Ouvre `http://localhost:8090` dans ton navigateur.

## Utilisation

### Ajouter un personnage

1. Clique sur **"+ Personnage"** dans l'interface
2. Clique sur **"Se connecter avec EVE Online"**
3. Autorise l'accès sur la page EVE
4. Le personnage est ajouté automatiquement

### Refresh des données

Clique sur **"Rafraîchir les prix"** pour mettre à jour les prix depuis ESI.

## Portabilité

Copie le dossier `bpo-dashboard/` entier (binaire + `data/`) sur n'importe quelle machine Linux. Aucune dépendance externe.

## Structure

```
bpo-dashboard/
├── target/release/bpo-dashboard  # Binaire
├── data/
│   ├── config.json               # Config avec credentials (NE PAS PARTAGER)
│   └── bpo-data-*.json           # Données par personnage (gitignored)
├── config.example.json           # Template de config (safe to share)
└── README.md
```

## Licence

MIT
