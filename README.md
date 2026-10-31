# Travel Dofus

[![Release](https://img.shields.io/github/v/release/ZainadosRepo/travel-dofus)](https://github.com/ZainadosRepo/travel-dofus/releases/latest)
[![CI](https://github.com/ZainadosRepo/travel-dofus/actions/workflows/ci.yml/badge.svg)](https://github.com/ZainadosRepo/travel-dofus/actions/workflows/ci.yml)

Petite fenêtre toujours au-dessus de Dofus 3 : on cherche un PNJ, une zone, un zaap, un repère de la carte (banque, atelier…) ou une position `x,y`, et l'appli copie le trajet le plus court (`/zaap x,y` répété par l'anti lag, puis `/travel x,y`). Avec « Ma position », elle compare la marche directe et le passage par un zaap.

Les données (2 080 PNJ placés par les quêtes, 47 zaaps, 834 repères, 15 409 cartes et le graphe de déplacement du jeu, noms dans les 5 langues) sont extraites des fichiers du client et embarquées dans l'appli : aucune connexion n'est nécessaire. L'interface reprend les 11 thèmes et la police (Lexend) de Dofus 3.

## Télécharger

Dernière version : **[Releases › Latest](https://github.com/ZainadosRepo/travel-dofus/releases/latest)**

| Système | Fichier |
| --- | --- |
| Windows 10 / 11 (64 bits) | `Travel.Dofus_<version>_x64-setup.exe` |
| macOS (Intel et Apple Silicon) | `Travel.Dofus_<version>_universal.dmg` |

### Windows

1. Lancer `Travel.Dofus_<version>_x64-setup.exe`.
2. L'installeur n'est pas signé : si Windows SmartScreen s'affiche, cliquer sur **Informations complémentaires**, puis **Exécuter quand même**.
3. Suivre l'installeur ; Travel Dofus apparaît dans le menu Démarrer.

### macOS

1. Ouvrir le `.dmg` et glisser **Travel Dofus** dans **Applications**.
2. L'appli n'est pas signée : au premier lancement, macOS la bloque. Ouvrir **Réglages Système › Confidentialité et sécurité**, puis cliquer sur **Ouvrir quand même** en bas de la page.

### Mettre à jour

Télécharger la nouvelle version et l'installer par-dessus l'ancienne. Les profils, favoris et réglages sont conservés.

## Utilisation

1. Ouvrir Travel Dofus à côté du jeu : la fenêtre, de la taille d'un panneau du jeu, reste au-dessus de Dofus. La glisser par son titre et la redimensionner par ses bords pour la caser dans l'interface.
2. Taper un nom de PNJ, de zone, de zaap ou de repère (accents et majuscules ignorés), ou une position comme `4,-18`.
3. Appuyer sur **Entrée** (ou cliquer sur le résultat) : le trajet est copié.
4. Dans Dofus, coller dans le chat (**Ctrl + V**) et valider.

Pour comparer la marche et le zaap, indiquer sa position actuelle dans **Ma position**.

Les distances suivent le graphe de déplacement du jeu : passages réels entre cartes, sens unique, murs et rivières qui coupent une carte. Elles sont comptées en cartes traversées. « Accès sous condition » signale un trajet qui passe par un accès réservé (quête, objet ou autre condition du jeu), comme pour une autre dimension ou certaines îles ; il n'est proposé que si aucun chemin libre n'existe.

| Action | Comment |
| --- | --- |
| Afficher / masquer la fenêtre | Ctrl + Alt + P, même depuis le jeu |
| Copier le résultat sélectionné | Entrée, ou clic sur la carte |
| Changer ce qui est copié (trajet, /zaap + /travel, /travel, /zaap, position, nom) | Tab / Maj + Tab, ou le bouton « Copie » |
| Favori (6 par profil) | ☆ sur un résultat ; recherche vide = liste des favoris, glisser pour réordonner |
| Copier un favori | Alt + 1 à 6 (ou 1 à 6 hors du champ de recherche) |
| Corriger un PNJ, choisir son zaap, rétablir | clic droit ou ⋯ sur un résultat |
| Profils, positions perso, anti lag, opacité, langue, thème | ⚙ |

La taille et la position de la fenêtre sont retenues. Les réglages sont dans `settings.json` :

- Windows : `%APPDATA%\fr.zainadosrepo.traveldofus\`
- macOS : `~/Library/Application Support/fr.zainadosrepo.traveldofus/`

Un fichier illisible est mis de côté en `settings.json.bad` au lieu d'être écrasé.

### Après une mise à jour de Dofus

Le pied de la fenêtre indique la version des données. Si Dofus installé est plus récent, un bandeau le signale : les PNJ ou zaaps ajoutés par le patch manquent jusqu'à la prochaine version de l'appli.

## Développement

Prérequis : Rust stable, Node (tests de l'interface) et la CLI Tauri 2 :

```bash
cargo install tauri-cli --version "^2" --locked
```

Lancer l'appli :

```bash
cargo tauri dev
```

### Tests

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
node --test "tests/ui/*.test.js"
```

`cargo test -p dofus-data -- --ignored` relance en plus une extraction complète sur l'install locale.

### Régénérer les données

```bash
cargo run --release -p dofus-data --bin extract
```

Relit l'install en lecture seule (`%LOCALAPPDATA%\Ankama\Dofus-dofus3` sous Windows, ou le dossier passé en argument) et réécrit `data/world.json` (quelques secondes).

### Publier une version

1. Monter la version dans `src-tauri/tauri.conf.json` et `src-tauri/Cargo.toml`.
2. Pousser un tag `v<version>` : `.github/workflows/release.yml` construit l'installeur Windows et l'image macOS dans une release en brouillon.
3. Relire la release sur GitHub et la publier.

## Avertissement

Projet de fan non officiel, sans lien avec Ankama. Dofus est une marque d'Ankama Games ; les données et les couleurs affichées proviennent de son client.
