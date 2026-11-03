# Equipe Codex no KanashaTerminal

## Decisão implementada

Pessoas gerenciadas pelo KanashaTerminal, com sessões independentes do Codex. O gerente delega através do MCP local. A interface real do Codex continua em um PTY por pessoa. A autenticação permanece no Codex; o aplicativo não pede API key.

A escolha permite que o cadastro, o nome e as responsabilidades de uma pessoa continuem existindo quando sua sessão estiver encerrada. O usuário controla a contratação e a interrupção. Subagentes nativos estão desabilitados nas sessões de equipe para manter essa correspondência.

## Como usar

1. Crie um Workspace. Ele recebe um gerente com nome provisório; na primeira conversa ou treinamento, o Codex gera e salva seu perfil profissional.
2. Envie treinamento ao gerente com processos, preferências e referências aos documentos. Ele cria o perfil e salva o aprendizado em uma memória individual.
3. Abra **Memória e treinamento**, confira/corrija o aprendizado e clique em **Revisado: concluir treinamento**. O gerente não pode aprovar a si próprio.
4. Contrate pessoas escolhendo a função e o nível/modelo. Com o gerente liberado, ele recebe automaticamente o pedido de integração, ensina o contratado e revisa sua memória antes de aprovar o treinamento.
5. Envie atribuições aos membros treinados e acompanhe a execução na Equipe ou no terminal. Conversas e documentos indicados continuam alimentando a memória por MCP.
6. Use **Treinamento complementar** para iniciar outra formação com revisão. Memórias também podem ser corrigidas diretamente pela interface.

Pessoas de versões anteriores preservam seus nomes e passam a aguardar treinamento. A memória de cada pessoa fica na configuração local do aplicativo, separada da conversa Codex. Alterações e aprovações exigem a revisão atual para evitar sobrescrever ou aprovar conhecimento que mudou durante a leitura. Remover uma pessoa também remove sua memória; duplicar a sala cria pessoas sem memória ou treinamento aprovado.

## Modelo por pessoa

| Nível | Modelo | Raciocínio padrão |
| --- | --- | --- |
| Júnior | GPT-6 Luna | xhigh |
| Pleno | GPT-5.6 Terra | xhigh |
| Sênior | GPT-6 Sol | xhigh |
| Especialista | GPT-6 Astra | high |
| Gerente | GPT-6 Sol | xhigh |

A contratação e a edição oferecem esses presets, seleção de modelo e raciocínio.
Combinações ajustadas viram Personalizado; o gerente mantém GPT-6 Sol. A configuração
é salva por pessoa e aplicada ao App Server, à abertura/retomada do thread, ao terminal
remoto e aos parâmetros de cada `turn/start`. Consulta ao catálogo e conferência da
resposta do Codex impedem substituição silenciosa na abertura. A interface distingue
configuração salva da confirmação recebida ao abrir a sessão.

Mudança de configuração requer encerrar a sessão conectada, preservando identidade,
memória, treinamento e ID da conversa. O encerramento cancela atribuições pendentes,
como já ocorria na edição de orientações. A nova configuração vale ao reabrir. Pessoas
antigas sem configuração passam a usar Sol xhigh. Alterações manuais via `/model` no
terminal podem mudar a conversa; a próxima atribuição reaplica a configuração salva.

Validação real: `installed_codex_applies_per_person_models_and_resumes_same_thread`
executou uma resposta `MODEL_PROFILE_OK` com Luna xhigh usando login ChatGPT e retomou
o mesmo thread com Terra xhigh, Sol xhigh e Astra high, verificando os parâmetros
confirmados e a preservação do histórico. As retomadas não executaram novas inferências.

```sh
cargo test --manifest-path src-tauri/Cargo.toml installed_codex_applies_per_person_models -- --ignored --nocapture
```

## Perfis profissionais personalizados

A função e o contexto recebido orientam o Codex a criar nome, personalidade e Markdown
com missão, competências de atuação, métodos, comunicação, critérios de qualidade, limites
e desenvolvimento no treinamento. A criação ocorre na própria conversa do gerente ou
contratado, pelo MCP `set_professional_profile`. Usa a autenticação existente do Codex.

O perfil fica visível na Equipe e salvo localmente com função e data de geração. A
personalidade já não é sorteada de um catálogo. Nomes provisórios ainda são locais até
a primeira conversa; nomes definidos pelo usuário e de cadastros anteriores são preservados.
`get_memory` retorna também o perfil e contexto da pessoa; `get_agent` inclui o perfil nos
detalhes, enquanto `get_team` continua resumido. Agora existem dez tools no MCP da Equipe.

O perfil é criado uma vez, com repetição idempotente do mesmo conteúdo. Cada sessão só
cria o seu. Mudar a função exige novo perfil e treinamento, mantendo o nome. Duplicar sala
não copia perfis ou aprendizados. O perfil não aprova treinamento nem é tratado como
experiência profissional comprovada; os processos efetivamente ensinados ficam na memória.

## Evidência de execução

Validação realizada com `codex-cli 0.158.0`, no macOS local.

| Verificação | Resultado |
| --- | --- |
| Login usado pela sessão real | `chatgpt` |
| Codex gera perfis distintos para gerente e QA pelo MCP | Passou: Caio e Lia, com métodos e limites próprios da função |
| Nome definido pelo usuário durante a contratação | Preservado: Lia |
| Perfis e nomes após retomar as sessões | Preservados sem nova geração |
| Gerente registra aprendizado e aguarda revisão humana | Passou |
| Tentativa de autoaprovação do gerente | Recusada |
| Gerente treina o contratado e revisa a memória por MCP | Passou |
| Regra ensinada pelo usuário aparece nas duas memórias | Passou |
| Gerente consulta equipe e delega por MCP | Passou |
| Membro executa e retorna resultado | Passou |
| Resultado chega ao gerente e inicia sua resposta | Passou |
| Repetição do mesmo recibo não duplica atribuição | Passou |
| Fechar e reabrir preserva o ID da sessão Codex | Passou |
| Interrupção é confirmada pelo evento do Codex | Passou |
| Instruções personalizadas da Área na primeira sessão | Passou: resposta inclui marcador definido somente no arquivo da área |
| Mudança externa e retomada com nova instrução | Passou: aviso pendente, mesmo ID de conversa e novo marcador na resposta |
| Testes Rust | 38 passaram; 2 testes dependentes do Codex ficam ignorados na suíte padrão |
| Teste dedicado de conexão e login com CLI instalado | Executado separadamente; passou |
| Testes React | 32 passaram (29 na suíte e 3 novos casos de seleção, todos incluídos no teste focado da Equipe) |
| TypeScript e build Vite | Passaram |
| Aplicativo macOS | Gerado e assinado ad hoc em `src-tauri/target/release/bundle/macos/KanashaTerminal.app` |

A jornada real roda somente com a opção explícita `--office-smoke` em build de desenvolvimento. Usa uma Área temporária isolada e não lê nem substitui o arquivo de configuração do usuário. Executa pedidos curtos e encerra suas sessões ao final. O relatório mais recente fica em `/tmp/kanasha-office-smoke-report.json`.

```sh
npm test
npm run build
cd src-tauri
cargo test
cargo test installed_codex_accepts_private_socket -- --ignored
cargo run -- --office-smoke
```

## Persistência e recuperação

### Instruções da Área

O template versionado do projeto está em `.kanasha/AGENTS.md`. Criar/selecionar uma
Área, trocar sua pasta-raiz ou iniciar uma sessão da Equipe prepara esse arquivo
quando ausente e acrescenta uma referência ao AGENTS.md da raiz. Conteúdo existente
é preservado e referências não são duplicadas. O botão **Instruções da Área** permite
editar o Markdown; o editor verifica a revisão antes de salvar. Limite: 64 KiB.

O conteúdo é fornecido explicitamente à sessão Codex com as instruções da pessoa.
A Equipe mostra a revisão carregada e avisa sobre mudanças nas atualizações da
interface e ao retornar o foco à janela. É necessário encerrar/reabrir a sessão
para aplicar mudanças; interromper somente o turno não recarrega o conteúdo.

A jornada real revelou que enviar `developerInstructions` em `thread/resume`
sozinho não bastava para aplicar a mudança no histórico existente da versão local
do Codex. A implementação usa também `thread/inject_items` para acrescentar a nova
orientação ao contexto, sem iniciar um turno extra. Um hash persistido evita
repetir a inclusão em reaberturas sem mudanças. O teste confirmou a troca de
`KANASHA_AREA_V1` por `KANASHA_AREA_V2`, preservando a conversa.

O [contrato do App Server](https://learn.chatgpt.com/docs/app-server) documenta a
inclusão de itens no histórico sem iniciar uma execução. A referência no AGENTS.md
raiz complementa o carregamento explícito; agentes externos ainda precisam de MCP
configurado e de acesso às instruções pelo próprio cliente.

### Sessões e tarefas

O cadastro, a personalidade, a memória, o estado de treinamento, os IDs de sessão, as atribuições e seus resultados são salvos localmente. Atividade e conexão atuais são transitórias. A interface não deduz trabalho em andamento apenas da existência de um processo.

Se o aplicativo terminar durante uma execução, o trabalho reaparece como incerto. Nenhuma repetição automática ocorre nesse estado. O usuário confere o histórico no terminal e encerra a execução pendente antes de atribuir novamente.

Uma pessoa interrompida pelo usuário não pode ser reativada pelo gerente. A fila dessa pessoa é cancelada. Resultados de membros continuam consultáveis se o gerente estiver interrompido.

## Limites desta versão

- Codex é o provedor da equipe. Os outros perfis continuam disponíveis como terminais comuns.
- A integração depende do protocolo App Server e da interface remota da versão instalada do Codex. A documentação oficial ainda trata partes dessa superfície como experimentais.
- Até 12 pessoas por sala e 10 atribuições pendentes por pessoa.
- As pessoas compartilham a pasta-raiz da Área. As especialidades são orientações; o gerente deve coordenar alterações concorrentes nos mesmos arquivos.
- O histórico completo da conversa fica no Codex. A tela Equipe guarda o trabalho enviado pelo aplicativo/MCP e seus resultados.
- Perguntas e aprovações de comando/arquivo são exibidas na Equipe. Outros tipos de solicitação interativa de plugins retornam um erro explícito ao agente.
- O protótipo HTML desta pasta continua sendo uma simulação histórica. A funcionalidade real está no aplicativo Tauri.
