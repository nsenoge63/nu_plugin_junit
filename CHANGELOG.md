
## [0.2.0] - 2026-08-14

### Added

### Changed

- Récupération du nom du fichier dans le champ `suite` pour les fichiers de tests non-Java (Jest, Mocha, pytest, etc.)
- Alignement à gauche des cellules plutôt que centrées
- Revue du README

### Fixed

---

## [0.1.0] - 2026-08-11

Version initiale du plugin

### Added

- Deux commandes  :
  - `junit report` : Parse tous les `*.xml` d'un répertoire de rapports surefire et retourne une table `{ suite, test, status, time }` (`time` en secondes).
  - `junit to-xlsx` : Consomme une table (typiquement la sortie de `junit report`, éventuellement filtrée/triée avant) et écrit un rapport Excel `.xlsx`

### Changed

### Fixed
