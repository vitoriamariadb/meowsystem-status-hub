# Integração com o MeowSystem

O que o MeowSystem precisa saber para compilar, instalar, configurar e vestir a **Bandeja**,
e o que falta do lado dele para a seção «Extensões».

| | |
| --- | --- |
| repositório | `https://github.com/vitoriamariadb/meowsystem-status-hub` (ramo `main`) |
| upstream | `https://github.com/marcelogomes90/cosmic-ext-applet-status-hub` (base: `2a0cb42`, 1.6.0) |
| app id | `io.github.meowsystem.Bandeja` |
| binários | `meowsystem-status-hub` (o applet), `meowsystem-status-hub-dump` (despeja a bandeja no terminal) |
| nome no painel | «Bandeja» (pt-BR), «Tray» (inglês) |
| licença | GPL-3.0-only, a do upstream |

## Compilar

Rust com edição 2024 (testado com 1.99.1) e as bibliotecas de desenvolvimento do COSMIC
(no Pop!_OS: `build-essential`, `pkgconf`, `libxkbcommon-dev`, `libwayland-dev`). O libcosmic
vem pinado no `Cargo.lock` (git `a401af8`); com `--locked` a compilação de amanhã é a de hoje.

```sh
cargo build --release --locked
cargo test --release --locked --features testkit   # os testes de D-Bus pedem o testkit
cargo clippy --release --locked --all-targets --all-features -- -D warnings
```

A compilação limpa leva uns 4 minutos; o binário tem uns 33 MB.

## Instalar (em `~/.local`, sem root)

`just install-user` faz isto:

| arquivo | destino |
| --- | --- |
| `target/release/meowsystem-status-hub` | `~/.local/bin/meowsystem-status-hub` |
| `resources/io.github.meowsystem.Bandeja.desktop`, com `Exec=` trocado pelo caminho absoluto do binário | `~/.local/share/applications/io.github.meowsystem.Bandeja.desktop` |
| `resources/io.github.meowsystem.Bandeja.svg` | `~/.local/share/icons/hicolor/scalable/apps/` |
| `resources/io.github.meowsystem.Bandeja-symbolic.svg` | `~/.local/share/icons/hicolor/scalable/apps/` |
| `resources/io.github.meowsystem.Bandeja.metainfo.xml` | `~/.local/share/metainfo/` |

O painel acha o applet pelo nome do `.desktop` (`X-CosmicApplet=true`) e só relê a lista ao
subir. Depois de instalar, a Bandeja aparece em Configurações › Área de trabalho › Painel ›
Applets; um applet já posto no painel só troca de binário quando o `cosmic-panel` reinicia.

Desfazer: `just uninstall-user` (apaga os cinco arquivos). A configuração fica em
`~/.config/cosmic/io.github.meowsystem.Bandeja/v1/`.

## Configuração (cosmic-config, versão 1)

Cada chave é um arquivo RON em `~/.config/cosmic/io.github.meowsystem.Bandeja/v1/<chave>`. O
applet vigia a pasta: mudar um arquivo por fora vale na hora, sem reiniciar nada.

| chave | tipo | padrão | o que faz |
| --- | --- | --- | --- |
| `pinned` | lista de `(id: "…", dup: 0)` | `[]` | itens fixados no painel, ao lado do botão |
| `order` | lista de `(id: "…", dup: 0)` | `[]` | ordem lembrada dos itens, inclusive dos ausentes |
| `colour-icons` | bool | `true` | `true`: ícones monocromáticos seguem a tinta do tema; `false`: cores originais |
| `hub-side` | `Left` \| `Right` | `Right` | de que lado da fileira fica o botão |
| `hub-icon` | `Applet` \| `Grid` \| `Dots` \| `Arrow` | `Applet` | ícone do botão (o próprio da Bandeja, ou `view-app-grid-symbolic`, `view-more-horizontal-symbolic`, `pan-down-symbolic` do tema) |
| `hide-passive` | bool | `false` | esconde os itens com `Status` `Passive` (o aplicativo diz que não há nada a ver) |
| `left-click` | `Auto` \| `Activate` \| `Menu` | `Auto` | `Activate` chama `Activate`; `Menu` abre o menu; `Auto` abre o menu quando o item publica `ItemIsMenu` ou não declara `Activate`, e ativa nos outros casos |

`id` é o `Id` do StatusNotifierItem (`busctl --user get-property <dono> /StatusNotifierItem
org.kde.StatusNotifierItem Id`, ou `meowsystem-status-hub-dump`); `dup` separa duas instâncias
do mesmo aplicativo.

**Migração do Status Hub.** Na primeira subida, cada chave de `pinned`, `order`,
`colour-icons` e `hub-side` que ainda não existe no id novo é copiada de
`~/.config/cosmic/io.github.marcelogomes90.cosmic-ext-applet-status-hub/v1/` ou, se não
houver, do flatpak (`~/.var/app/<id antigo>/config/cosmic/<id antigo>/v1/`). Nada é apagado no
id antigo, e uma chave que já existe no novo nunca é sobrescrita.

## Extensão de tema

Sem extensão, tudo segue o tema do COSMIC. Com ela, o pack põe cores e ícones por cima.

- **Arquivo:** `$XDG_CONFIG_HOME/meowsystem/extensoes/io.github.meowsystem.Bandeja/tema.conf`
  (`~/.config/…` sem `XDG_CONFIG_HOME`). A variável `MEOWSYSTEM_TEMA` aponta outro arquivo.
- **Quando é lido:** ao subir e a cada vez que o popup abre. Trocar de pack vale no próximo
  clique no botão; o painel não precisa reiniciar.
- **Formato:** uma `chave = valor` por linha, `#` começa comentário, espaço em volta é
  ignorado. Cor é `#rrggbb` ou `#rrggbbaa`; nome de paleta (`mauve`) quem traduz é o MeowSystem.
- **Falhas:** arquivo ausente é o tema do COSMIC; arquivo ilegível ou linha ruim (cor
  inválida, chave desconhecida, nome de ícone com `/`) vira aviso no journal e as outras linhas
  valem. Nada disso derruba o applet.

| chave | valor | onde aparece |
| --- | --- | --- |
| `tinta` | cor | glifos simbólicos (botão, ícones das linhas) e a cor que recolore os ícones monocromáticos dos aplicativos |
| `fundo` | cor | fundo do cartão do popup; também a base do contraste ao recolorir |
| `borda` | cor | contorno do cartão do popup |
| `texto` | cor | texto do popup |
| `destaque` | cor | fundo do item cujo menu está aberto |
| `acento` | cor | engrenagem do popup, realce dos ícones da bandeja ao passar o mouse e ícones dos títulos de seção nas configurações |
| `icone-botao` | nome de ícone | ícone do botão no painel; vence a preferência `hub-icon` |
| `icone.<Id>` | nome de ícone | troca o ícone do item cujo `Id` é `<Id>` (no popup, no painel e nas configurações) |

Nome de ícone é um nome do tema de ícones em uso (letras, dígitos, `-`, `_`, `.`), nunca
caminho. Nome terminado em `-symbolic` é pintado com a `tinta`.

Exemplo (Catppuccin Mocha, acento mauve):

```ini
# gerado pelo MeowSystem a partir do pack
tinta = #cdd6f4
fundo = #1e1e2ef0
borda = #45475a
texto = #cdd6f4
destaque = #313244
acento = #cba6f7
icone-botao = view-app-grid-symbolic
icone.spotify-client = spotify-indicator
icone.hefesto-dualsense4unix = input-gaming-symbolic
```

A mesma convenção (pasta, formato, chaves de cor) vale para o Ajustes rápidos
(`io.github.meowsystem.AjustesRapidos`); o MeowSystem pode gerar os dois com o mesmo código.

## O que o MeowSystem precisa para instalar pela seção «Extensões»

Isto é a sprint do lado do MeowSystem; nada aqui foi mudado nele.

1. **Pino.** Um arquivo como o do now-playing: `URL=https://github.com/vitoriamariadb/meowsystem-status-hub`,
   `COMMIT=<hash do main>`, `LIBCOSMIC=a401af8`.
2. **Receita de build** no molde do `midia_build.sh`: clone em `~/.local/state/meowsystem/`,
   `git checkout <COMMIT>`, `cargo build --release --locked`, carimbo com o commit para a
   segunda passada não recompilar, e a cópia da tabela de «Instalar» (ou `just install-user`).
3. **Tema.** Na troca de pack, escrever o `tema.conf` acima a partir da paleta e de um
   `packs/<pack>/extensoes/io.github.meowsystem.Bandeja/` opcional com os ícones por item. Não
   precisa avisar o applet.
4. **Painel.** Trocar no painel a entrada `io.github.marcelogomes90.cosmic-ext-applet-status-hub`
   por `io.github.meowsystem.Bandeja` e reiniciar o `cosmic-panel`. Hoje a ordem do painel é de
   quem usa: a interface oferece a troca e só a faz com o sim dela.
5. **Doctor.** Conferir binário, `.desktop`, carimbo = pino e o `tema.conf` do pack em uso.
6. **Desinstalar.** `just uninstall-user` mais a pasta do clone; a configuração fica.
