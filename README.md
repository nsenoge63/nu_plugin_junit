# nu_plugin_junit

Plugin **nushell natif** (protocole `nu_plugin_*`, écrit en Rust) qui parse
des rapports JUnit/surefire et produit un rapport Excel stylé — directement
dans le pipeline nushell, sans JVM, sans jar, sans fichier intermédiaire.

Port du projet Java d'origine (`JunitReportGeneratorMain` / `XmlParser` /
`ExcelGenerator`), avec un vrai changement de paradigme : plutôt que
"générer un fichier", le plugin retourne d'abord une **table structurée**
que tu peux filtrer/trier/agréger avec les commandes nushell natives avant
de l'exporter.

## Commandes

### `junit report [path]`

Parse tous les `*.xml` d'un répertoire de rapports surefire et retourne une
table `{ suite, test, status, time }` (`time` en secondes).

```nu
junit report ./target/surefire-reports
# ou, avec le défaut (target/surefire-reports relatif au dossier courant) :
junit report
```

Comme c'est une table nushell normale, tout le reste de nushell s'applique
sans rien écrire de plus :

```nu
junit report | where status == "failed"
junit report | group-by status | transpose status cases | insert count {|r| $r.cases | length}
junit report | sort-by time --reverse | first 5
junit report | to csv | save resultats.csv
junit report | to json | save resultats.json
```

### `junit to-xlsx <path> [--project <nom>] [--branch <branche>]`

Consomme une table (typiquement la sortie de `junit report`, éventuellement
filtrée/triée avant) et écrit un rapport Excel `.xlsx` stylé : classes de
test fusionnées, statuts colorés, formules `COUNTIF` pour les totaux, durée
totale, pied de page projet/branche.

```nu
junit report ./target/surefire-reports
  | junit to-xlsx ./rapport.xlsx --project mon-projet --branch main
```

## Build

```bash
cargo build --release
# binaire : target/release/nu_plugin_junit
```

### Note de compilation

Aucune dépendance C système : `rust_xlsxwriter` utilise par défaut une
implémentation deflate 100% Rust (la feature optionnelle `zlib`, plus
rapide mais nécessitant un compilateur C, n'est pas activée ici). `roxmltree`
est également du Rust pur. Ça simplifie le cross-compilation (cf. section
"Publier une release" plus bas) : pas de toolchain C à installer par cible.

Par ailleurs, `nu-plugin-core 0.108.0` déclare une dépendance
souple sur `interprocess >= 2.2.0`, mais les versions `interprocess` plus
récentes que 2.2.0 ont cassé une API qu'il utilise. **Le `Cargo.lock` fourni
épingle déjà `interprocess = 2.2.0`**, donc un simple `cargo build` sans y
toucher fonctionne. Si tu mets à jour les dépendances (`cargo update`) et
que ça recasse, refais :

```bash
cargo update -p interprocess --precise 2.2.0
```

Ou, plus simple, ignore complètement le souci en gardant
`default-features = false` sur `nu-plugin` dans `Cargo.toml` (ce qui est
déjà fait) : ça désactive la feature `local-socket` qui tire `interprocess`,
et le plugin communique en stdio classique — largement suffisant ici.

## Installation dans nushell

```nu
plugin add ./target/release/nu_plugin_junit
plugin use junit
```

Ensuite `junit report` et `junit to-xlsx` sont disponibles dans toutes tes
sessions nushell (l'enregistrement est persistant, pas besoin de refaire
`plugin add` à chaque lancement).

## Tests

```bash
cargo test
```

Les tests utilisent `nu-plugin-test-support` (`PluginTest`), qui fait
tourner un vrai moteur/parseur nushell **en process**, sans passer par le
protocole stdio ni un binaire `nu` externe — c'est le moyen recommandé de
tester un plugin nushell. Trois tests fournis :
- parsing d'un répertoire de rapports en table,
- gestion d'erreur sur un répertoire inexistant,
- pipeline complet `junit report | junit to-xlsx` avec vérification du
  fichier `.xlsx` produit (archive OOXML valide, cellules et fusions
  correctes — vérifié manuellement avec `openpyxl` pendant le
  développement).

## Différences avec la version Java d'origine

- **Format `.xlsx` (OOXML)** au lieu de `.xls` (binaire HSSF). C'est un
  changement volontaire : les bibliothèques Rust matures pour Excel ciblent
  l'OOXML, et `.xlsx` est de toute façon le format natif d'Excel depuis
  2007.
- **Pas d'ouverture automatique du fichier** (`Desktop.open()` côté Java) :
  ça n'a pas de sens dans un pipeline nushell — utilise `open ./rapport.xlsx`
  toi-même si tu veux l'ouvrir.
- **Couleurs approximées** : les couleurs indexées HSSF d'origine
  (`IndexedColors.BRIGHT_GREEN1`, etc.) sont approximées en RGB, pas
  identiques au pixel près.
- Le nom du projet et la branche git ne sont plus déduits automatiquement
  d'un `.git/HEAD` (le plugin n'a pas connaissance d'un "répertoire de
  projet" comme le faisait le CLI Java) : passe-les explicitement avec
  `--project` / `--branch` si tu veux les voir dans le pied de page. Pour
  les récupérer automatiquement dans un script nushell :

  ```nu
  junit report
    | junit to-xlsx ./rapport.xlsx --project (basename (pwd)) --branch (git branch --show-current)
  ```

## Publier une release (build multi-plateformes)

Le workflow fourni dans `.github/workflows/release.yml` construit le
plugin pour 5 cibles (Linux x86_64/aarch64, macOS Intel/Apple Silicon,
Windows x86_64) et publie chaque binaire en asset d'une release GitHub dès
qu'un tag `vX.Y.Z` est poussé :

```bash
git tag v0.1.0
git push origin v0.1.0
```

Chaque asset est nommé `nu_plugin_junit-<target-triple>.(tar.gz|zip)`,
ex. `nu_plugin_junit-x86_64-unknown-linux-gnu.tar.gz` — c'est la convention
de nommage que le backend `github` de mise sait reconnaître automatiquement
pour choisir le bon binaire selon l'OS/l'architecture de la machine.

La cible `aarch64-unknown-linux-gnu` est cross-compilée via
[`cross`](https://github.com/cross-rs/cross) (pas de runner GitHub natif
pour cette architecture) ; les autres sont compilées nativement sur leur
plateforme. Comme le projet n'a aucune dépendance C système (voir la note
de compilation plus haut), le cross-build ne demande aucune configuration
supplémentaire.

## Intégration mise

```toml
[tools]
"github:VOTRE_ORG/nu_plugin_junit" = "latest"
```

```bash
mise use "github:VOTRE_ORG/nu_plugin_junit@latest"
```

mise télécharge et installe le bon binaire pour la plateforme courante et
l'expose sur le PATH. Il reste ensuite une seule étape, à faire une fois,
**dans nushell** (mise ne peut pas le faire à ta place : c'est nushell qui
gère la liste de ses plugins enregistrés, indépendamment de mise) :

```nu
plugin add (which nu_plugin_junit | get path.0)
plugin use junit
```

Cet enregistrement est persistant d'une session nushell à l'autre — pas
besoin de le refaire à chaque ouverture de terminal, seulement après une
mise à jour du binaire.


