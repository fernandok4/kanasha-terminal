# KanashaTerminal

Aplicativo desktop local para organizar agentes de terminal em paralelo. A interface está em PT-BR e funciona em Linux e macOS por meio de Tauri, Rust e WebView.

## O que o MVP oferece

- Áreas de Trabalho vinculadas a uma pasta-raiz.
- Workspaces em abas dentro de cada Área.
- Vários terminais em painéis com divisões horizontais ou verticais.
- Divisores arrastáveis e ajustáveis por teclado, com proporções restauradas localmente.
- Foco rápido entre painéis, maximização temporária, nomes e etiquetas visíveis para identificar cada agente.
- Perfis internos para Shell, Codex, Claude e Gemini, quando disponíveis no `PATH`, além de perfis customizados.
- Processos que continuam ativos ao trocar de Área ou Workspace.
- Restauração local de Áreas, Workspaces, layouts, painéis e metadados de perfis. Terminais restaurados ficam parados e podem ser iniciados manualmente.
- Menus de contexto, atalhos de teclado, tooltips e confirmação antes de encerrar processos.

O terminal sempre inicia na pasta-raiz da Área. Os dados ficam somente no diretório de configuração do aplicativo e não há servidor HTTP, WebSocket ou porta de controle.

## Privacidade da persistência

O aplicativo **não persiste** buffers ou saída de terminal, comandos executados, argumentos de perfil, tokens, segredos, variáveis de ambiente, PIDs ou sessões. Para proteger essa fronteira, o executável e os argumentos de um perfil customizado são mantidos somente em memória: devem ser informados novamente após reiniciar o aplicativo.

## Pré-requisitos

O projeto requer Node.js `^20.19.0` ou `>=22.12.0`, com npm, e o Rust instalado pelo `rustup`.

### macOS

Para desenvolvimento desktop, instale as ferramentas de linha de comando do Xcode:

```bash
xcode-select --install
```

Instale o Rust e abra um novo terminal para que `cargo` seja adicionado ao `PATH`:

```bash
curl --proto '=https' --tlsv1.2 https://sh.rustup.rs -sSf | sh
```

Se quiser continuar no terminal atual logo após a instalação:

```bash
source "$HOME/.cargo/env"
```

### Ubuntu/Debian

Instale as dependências nativas do Tauri:

```bash
sudo apt update
sudo apt install libwebkit2gtk-4.1-dev \
  build-essential \
  curl \
  wget \
  file \
  libxdo-dev \
  libssl-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev
```

Depois instale o Rust e abra um novo terminal:

```bash
curl --proto '=https' --tlsv1.2 https://sh.rustup.rs -sSf | sh
```

## Desenvolvimento

Os mesmos comandos iniciam o projeto no macOS e no Ubuntu/Debian:

```bash
npm install
npm run tauri dev
```

Se aparecer `failed to run 'cargo metadata' ... No such file or directory`, o Rust ainda não está instalado ou o terminal atual ainda não carregou `~/.cargo/bin` no `PATH`. Abra outro terminal ou execute `source "$HOME/.cargo/env"`.

## Instalador para macOS

O DMG precisa ser gerado em um Mac. A arquitetura do instalador será a mesma da máquina usada no build (Apple Silicon ou Intel):

```bash
npm install
npm run tauri build -- --bundles app,dmg
```

Os artefatos ficam em:

- aplicativo: `src-tauri/target/release/bundle/macos/KanashaTerminal.app`
- instalador: `src-tauri/target/release/bundle/dmg/*.dmg`

Abra o DMG e arraste `KanashaTerminal` para `Applications`. O build local recebe uma assinatura ad hoc, mas não é publicado, notarizado nem assinado com um certificado Apple Developer. Ao distribuir o arquivo para outro Mac, o Gatekeeper pode exigir autorização em **Ajustes do Sistema > Privacidade e Segurança**; distribuição pública requer assinatura e notarização.

Se o build parar em `Running bundle_dmg.sh`, permita que o aplicativo de terminal usado no build controle o Finder em **Ajustes do Sistema > Privacidade e Segurança > Automação** e execute o comando novamente.

## Validação

```bash
npm test
npm run build
cd src-tauri && cargo test
npm run tauri build
```

O último comando gera, localmente, os pacotes compatíveis com o sistema em que o build foi executado, dentro de `src-tauri/target/release/bundle/`. Ele não publica, assina com certificado de distribuição ou faz deploy.

## Atalhos

- `Ctrl/Cmd + N`: cria um novo terminal no workspace atual.
- `Ctrl/Cmd + Shift + N`: abre outro terminal mesmo quando um terminal está com foco.
- `Ctrl/Cmd + Shift + W`: cria um novo workspace na Área atual.
- `Ctrl/Cmd + ,`: abre a criação de perfil customizado.
- `Ctrl/Cmd + Shift + A`: abre a criação de Área de Trabalho.
- `Ctrl/Cmd + Alt + ←/↑` e `Ctrl/Cmd + Alt + →/↓`: muda o foco para o painel anterior/próximo, inclusive a partir do terminal.
- `Ctrl/Cmd + Shift + C` e `Ctrl/Cmd + Shift + V`: copiam a seleção e colam texto no terminal, pelos controles nativos do xterm/WebView.

Os atalhos são locais à janela. Os de criação não interceptam a entrada do terminal quando o painel está com foco; a navegação entre painéis foi deliberadamente mantida disponível nesse contexto.
