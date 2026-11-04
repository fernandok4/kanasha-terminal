# KanashaTerminal

Aplicativo desktop local para organizar agentes de terminal em paralelo. A interface está em PT-BR e funciona em Linux e macOS por meio de Tauri, Rust e WebView.

## Instruções concisas para a equipe

O template v17 de `.kanasha/AGENTS.md` usa atribuições com objetivo, escopo,
referências e aceite; retornos com resultado, evidências e pendências. Os prompts
injetados foram condensados, com consultas sob demanda e atualizações nas transições.
Os contratos de aprovação, filas e conferência de entregas continuam vigentes.

Alma guarda princípios duradouros em seções curtas; memória funciona como índice de
lembranças datadas com referências. As orientações sugerem 2.000 caracteres para alma,
4.000 para memória e 1.500 para mensagens, permitindo expansão quando necessária.
Esses valores são orientações, não truncamento nem alteração dos limites técnicos.
Conteúdo já salvo não é reescrito automaticamente. Áreas com AGENTS personalizado
são preservadas; áreas com equipes recebem o template atual. Recarregue a sessão para
aplicar os prompts atualizados.

## O que o MVP oferece

- Áreas de Trabalho vinculadas a uma pasta-raiz.
- Workspaces em abas dentro de cada Área, com escolha entre **Organização normal** (padrão, terminais livres) e **Equipe** (gerente e contratação), inclusive ao criar a primeira área. O menu Equipe aparece somente nesse tipo. Workspaces antigos com membros são preservados como equipes; os demais continuam normais.
- Modo opcional **Escritório**: mapa top-down da Área, com uma baia identificada por workspace do tipo Equipe e uma mesa por funcionário. Clique na pessoa para abrir seu terminal; use **Equipe** na placa da baia para gerenciar o time. Alternar a visualização preserva os terminais montados e as sessões ativas. A interface tradicional continua sendo a inicial.
- Equipe por Workspace: gerente, contratação de pessoas com nome, especialidade e orientações editáveis, terminais Codex próprios e atribuições com resultados.
- Resumo persistente por Workspace e status individual dos agentes, atualizados via MCP local.
- Vários terminais em painéis com divisões horizontais ou verticais.
- Divisores arrastáveis e ajustáveis por teclado, com proporções restauradas localmente.
- Foco rápido entre painéis, maximização temporária, nomes e etiquetas visíveis para identificar cada agente.
- Perfis internos para Shell, Codex, Claude e Gemini, quando disponíveis no `PATH`, além de perfis customizados.
- Processos que continuam ativos ao trocar de Área ou Workspace.
- Restauração local de Áreas, Workspaces, layouts, painéis e metadados de perfis. Terminais restaurados ficam parados e podem ser iniciados manualmente.
- Menus de contexto, atalhos de teclado, tooltips e confirmação antes de encerrar processos.

O terminal inicia na pasta-raiz da Área. A integração MCP usa STDIO e um socket Unix local protegido. As sessões da equipe usam o App Server do Codex com WebSocket sobre socket Unix em diretório privado, sem abrir uma porta TCP. O próprio Codex acessa os serviços necessários à execução com a autenticação local existente.

## Sua equipe com Codex

O KanashaTerminal gerencia as pessoas; o Codex executa cada sessão. Não é necessário fornecer uma API key ao KanashaTerminal. Use uma instalação do Codex com login ChatGPT (`codex login`) e suporte a `app-server --listen unix://...` e `codex --remote ... resume`. A conta continua sujeita aos seus limites de uso.

1. Crie um Workspace: ele recebe um **Gerente** com nome definido pela aplicação e editável por você. Na primeira conversa ou treinamento, o próprio Codex cria e salva personalidade e perfil profissional, preservando esse nome.
2. Em **Enviar treinamento**, ensine seus processos ao gerente e indique documentos. Ele consolida identidade, missão e responsabilidades na proposta de alma; aprendizados posteriores ficam na memória. Você também pode treiná-lo no terminal: ele precisa salvar o aprendizado por MCP e concluir o turno.
3. Abra **Alma e treinamento**, revise/corrija alma e memória e escolha **Revisado: concluir treinamento**. Só o usuário libera o gerente; ele não pode aprovar a si próprio.
4. Use **Contratar agente** e escolha a função (Desenvolvedor, QA, PO ou uma especialidade livre) e o nível profissional. A aplicação define o nome na contratação; o Codex cria personalidade e perfil profissional durante a primeira conversa ou treinamento, considerando a função e o contexto recebido. Com o gerente treinado, a contratação agenda a integração: o gerente ensina o contratado, revisa sua alma e memória e o libera. Isso executa sessões Codex e utiliza os limites normais da conta. Enquanto o gerente não estiver liberado, os contratados aguardam.
5. Depois do treinamento, envie objetivos em **Enviar atribuição**. A tela Equipe mostra atividade, histórico e perguntas; **Abrir terminal** permite conversar na mesma sessão.
6. **Interromper trabalho** cancela o turno e a fila da pessoa. **Editar pessoa** permite ajustar nome e orientações, encerrando a sessão após confirmação. Mudar a especialidade exige novo treinamento. **Retirar da equipe** remove cadastro, memória e atribuições locais, preservando os arquivos do projeto.

Workspaces existentes continuam com seus painéis e nomes. Pessoas sem perfil gerado o criam na próxima conversa, preservando o nome existente e o estado de treinamento salvo; não se presume novo aprendizado. Na Equipe, a primeira contratação estabelece o gerente. Duplicar um Workspace copia nomes, funções e orientações, criando novos IDs, sem copiar almas, perfis profissionais, sessões, atribuições, memórias ou treinamento concluído. Os nomes copiados são preservados ao criar os novos perfis.

O QA recebe como orientação inicial testar regras de negócio e jornadas com
kanasha-flow, registrando critérios, resultados e evidências. O gerente ensina os
processos, ferramentas e referências da empresa; o funcionário propõe sua alma e
aguarda revisão. Não há alma pré-aprovada por função. Cadastros existentes preservam
suas orientações e precisam de edição ou treinamento complementar para incorporá-las.

O gerente pode usar `release_task {terminalId, taskCode, assignmentId}` para
encerrar a participação individual após conferir a entrega, inclusive quando
um step compartilhado continua com outra pessoa ou houve vínculo por engano.
Não exige concluir esses steps e preserva seus status. Exige a última atribuição
concluída, sem atribuições próprias pendentes nessa tarefa, turno ativo, pergunta
ou resultado incerto. X continua aberta;
Y pode iniciar na fila do autor. Correções futuras em X voltam ao fim da fila,
sem interromper Y. Vínculos e histórico são preservados; a tela Equipe e o MCP
mostram as participações liberadas. Não usa o `paused` do funcionário nem conclui
a tarefa antes da revisão. O `assignmentId` da última entrega impede uma repetição
atrasada de liberar novas correções.

### Entregas e evidências

Os funcionários usam `register_work_context` para registrar os checkouts de cada
atribuição e `submit_delivery` para descrever a entrega, referências, versões,
testes e limitações. O aplicativo coleta Git local no registro e na entrega,
sem fetch, push ou execução dos testes declarados. Essa observação não comprova
autoria, especialmente em checkout compartilhado. Registro tardio não reconstrói
a situação inicial; diffs rastreados não incluem conteúdo untracked/submódulos.

O agente gerente usa `get_deliveries` por tarefa ou atribuição, confere referências
com as ferramentas existentes e registra `review_delivery` antes de atualizar
resumo e Jira. Cada evidência distingue declarada, conferida pelo gerente,
indisponível, divergente e não conferida. Uma MR informada não é automaticamente
validada. GitLab inacessível não significa ausência de alteração.

Entregas e conferências têm versões e recibos idempotentes. Novas entregas não
herdam aceite da anterior. A tela Equipe mostra versões, checkouts, evidências e
histórico de conferências. Tarefas antigas continuam legíveis, sem evidência
retroativa inventada. Esses registros não alteram fila, Jira ou status da tarefa;
a liberação individual continua independente de um step compartilhado.

### Revisão entre desenvolvedores

O template v16 orienta o gerente a delegar implementação e revisão independente
como steps da mesma tarefa Jira. O revisor confere guardrails, critérios de aceite
e impacto na regra de negócio. A revisão só é enviada após a entrega do autor:
a ordem dos steps não bloqueia automaticamente a fila de outro funcionário.

O padrão é uma revisão inicial e, se necessário, uma rodada de correções com
rechecagem focada. O gerente registra rodadas, evidências e achados em
`equipes/<workspace-id>/tasks/<task>/review.md`. Sem bloqueantes e com os critérios
atendidos, conclui; se o limite for atingido com impedimentos, registra a pendência
e pede uma decisão, sem repetir indefinidamente nem declarar uma entrega incompleta.
Esse limite é orientação ao gerente, não um contador imposto pelo motor da fila.

### Modelo e raciocínio por pessoa

Na contratação ou em **Editar pessoa**, escolha o nível para preencher a configuração:

| Nível | Modelo | Raciocínio |
| --- | --- | --- |
| Júnior | `gpt-6-luna` | `xhigh` |
| Pleno | `gpt-5.6-terra` | `xhigh` |
| Sênior | `gpt-6-sol` | `xhigh` |
| Especialista | `gpt-6-astra` | `high` |
| Gerente | `gpt-6-sol` | `xhigh` |

Contratados e gerente permitem escolher qualquer modelo visível retornado pelo
catálogo `model/list` do Codex instalado, incluindo novas versões e famílias sem
atualizar uma lista fixa no KanashaTerminal. O seletor de raciocínio acompanha os
esforços suportados por cada modelo. **Atualizar modelos** consulta novamente o
catálogo; falhas preservam a configuração salva e permitem tentar de novo.
Os níveis são presets de execução e orientações de
atuação, não uma garantia de experiência ou de resultado.

Modelo e raciocínio são persistidos por pessoa e enviados como parâmetros reais ao
Codex na criação/retomada da conversa, no terminal remoto e em cada atribuição.
A consulta do catálogo não cria conversa nem executa inferência. A presença na lista
não garante acesso a uma inferência pela conta, e a lista pode diferir da do ChatGPT.
A aplicação consulta `model/list` e verifica a confirmação do Codex ao abrir a sessão.
Configuração indisponível ou resposta diferente gera erro, sem substituir o modelo.
A Equipe mostra a configuração salva e a confirmação recebida ao abrir a sessão.

Editar uma pessoa conectada solicita encerrar sua sessão antes de aplicar a mudança,
preservando o ID da conversa, perfil, memória e treinamento. Ao reabrir, a nova
configuração é aplicada. O encerramento segue a regra existente de cancelar trabalho
pendente: faça a troca quando a pessoa estiver ociosa. Cadastros antigos sem configuração
recebem Sol xhigh (Sênior para membros e Gerente para o coordenador).

As sessões continuam usando o login local do Codex, inclusive ChatGPT, sem API key.
Comandos manuais de troca de modelo dentro do TUI podem alterar aquela conversa; as
próximas atribuições do KanashaTerminal reaplicam a configuração salva da pessoa.

### Perfil profissional criado pelo Codex

Você escolhe a função. Na primeira conversa ou treinamento, a pessoa consulta o contexto
da empresa e a equipe, cria um perfil personalizado e salva por `set_professional_profile`.
A geração acontece na própria sessão Codex com o login existente, consumindo seus limites
normais de uso. Não há chamada separada por API key. Antes dessa conversa, a interface
identifica o perfil pendente. O nome exibido já está definido e só você pode alterá-lo pela interface; criar o perfil preserva esse nome, inclusive em cadastros antigos com `automaticName: true`.

O perfil contém missão, competências de atuação, forma de trabalhar, comunicação,
critérios de qualidade, limites e pontos a desenvolver no treinamento. É uma orientação de
comportamento, sem inventar histórico profissional ou processos que ninguém ensinou.
Ele fica visível em **Perfil profissional · criado pelo Codex**, salvo no `workspace.json`
com função e data, e é incluído no contexto das sessões e próximas atribuições.

Cada pessoa cria somente seu próprio perfil, uma vez. Repetir exatamente o conteúdo é
seguro; substituir um perfil existente é recusado. Nomes escolhidos pelo usuário são
preservados. Alterar a função invalida o perfil e exige novo treinamento, mantendo o nome.
Se a geração não terminar, o perfil permanece pendente; a próxima conversa consulta o
estado antes de tentar novamente. Reabrir a sessão não sorteia outra identidade.

O perfil tem até 6.000 caracteres Markdown e a personalidade até 800 caracteres.
A alma consolidada no treinamento e aprovada define a identidade permanente e prevalece sobre o perfil inicial. Aprendizados posteriores ficam na memória; gerar o perfil não aprova treinamento.
As orientações explícitas do usuário prevalecem sobre o perfil.

### Alma, treinamento e memória

**Alma** define quem a pessoa é, missão, responsabilidades, valores, modo de trabalhar,
comunicação, critérios de decisão e limites, a partir do treinamento inicial. Fica salva
separadamente no `workspace.json`, até 12.000 caracteres Markdown, com revisão e autoria da
edição e aprovação. A tela **Alma e treinamento** mostra a proposta e a última identidade
aprovada. Salvar uma proposta não a aprova: a identidade anterior permanece vigente até revisão.
O nome cadastrado continua sendo editado somente pelo usuário na interface.

**Memória** guarda aprendizados posteriores, preferências observadas, decisões e contexto
útil das demandas, até 16.000 caracteres Markdown, com revisão e autor. Registra também
experiências, como treinamentos e entregas: prefira lembranças de uma a três frases, com
data explícita, resultado confirmado e referência a arquivos reais para detalhes. Cada
funcionário mantém suas próprias lembranças; detalhes pessoais podem ficar em
`equipes/<workspace-id>/memorias/<terminal-id>/` na raiz da Área. Os agentes consultam
os arquivos pertinentes ao assunto, mantendo lembranças recentes em destaque e antigas
resumidas com referências. Isso é orientação do template v14, não seleção automática
por idade. Ela é independente
da alma e do histórico do Codex. **Histórico de demandas** conserva andamento, resultados e
pendências na tarefa e atribuições. Atualizações de memória não alteram a alma.

Fluxo: **Aguardando treinamento → Em treinamento → Aguardando revisão → Treinamento concluído**.
No treinamento inicial, o agente consolida a alma por `set_soul`; não precisa copiá-la para
`set_memory`. O término de uma conversa só permite revisão quando houve novo registro de
alma ou memória. A conclusão exige alma consolidada, revisão exata da alma e memória e
nenhuma execução pendente. O agente registra na memória uma lembrança curta do treinamento, sem copiar a alma inteira; a aplicação permite revisar a formação mesmo sem aprendizados posteriores.
Só você aprova a formação do gerente; o gerente treinado pode aprovar a dos membros.

A alma aprovada entra nas sessões novas/retomadas e no contexto antes das próximas
atribuições. A proposta pendente é identificada como tal. O agente consulta `get_memory`
ou `get_soul` ao retomar e não pode redefinir a alma durante trabalho normal.
**Treinamento complementar** permite nova proposta, preservando a versão aprovada até
revisão. Você também pode editar e aprovar a alma separadamente pela interface, sem
confundir essa ação com conclusão de treinamento.

Cadastros anteriores preservam perfil, memória, histórico e estado de treinamento. Não
convertem automaticamente esses textos em uma alma aprovada; consolide e revise a identidade
na interface ou use treinamento complementar. Pessoas já liberadas continuam liberadas.
Mudar a especialidade exige nova alma e treinamento. Duplicar a sala não copia almas.
Instruções comuns da empresa continuam em `.kanasha/AGENTS.md`; esta mudança não cria
um campo de responsabilidade coletiva do time.

Se a integração não iniciar porque o gerente está interrompido, há uma execução incerta ou a fila
está cheia, o contratado permanece cadastrado. Resolva o impedimento e use **Solicitar treinamento
ao gerente**. Interromper preserva os aprendizados; retirar a pessoa remove também sua memória local.
As personalidades variam a comunicação e organização, mantendo os mesmos requisitos de qualidade.

As especialidades são instruções flexíveis, não permissões de sistema. Cada pessoa usa uma sessão independente; a delegação nativa de subagentes fica desabilitada nessas sessões para que a equipe registrada corresponda às pessoas gerenciadas pelo aplicativo. Nesta versão, há até 12 pessoas por sala e até 10 atribuições pendentes por pessoa. Todas usam a pasta-raiz da Área; o gerente deve coordenar trabalhos que alterem os mesmos arquivos.

O gerente recebe os resultados por uma nova atribuição interna, inclusive quando estava ocioso. Resultados continuam visíveis no histórico do membro se o gerente estiver interrompido. Uma entrega sem confirmação aparece como **Verificar na sessão** e bloqueia novos envios automáticos para essa pessoa. Confira o terminal e use **Encerrar execução pendente**, depois disponibilize a pessoa novamente. O aplicativo não repete automaticamente uma execução incerta.

Ferramentas adicionais das sessões de equipe:

| Tool | Comportamento |
| --- | --- |
| `get_tasks {}` | Tarefas Jira locais e vínculos com funcionários, atribuições e steps. |
| `get_team {}` | Pessoas da sala, especialidades, disponibilidade, atividade e filas. |
| `get_agent {terminalId}` | Orientações, execução atual, fila em ordem e última atribuição/resultado da pessoa. O histórico completo fica na tela Equipe. |
| `set_assignment {terminalId, requestId, message, taskCode?, stepIds?, freshContext?, training?}` | Gerente enfileira trabalho, inclusive para pessoa ocupada; `training: true` inicia formação. O mesmo recibo e conteúdo não geram outra execução. |
| `reset_context {terminalId, requestId}` | Gerente inicia conversa nova de outro funcionário ocioso da sala, preservando alma, lembranças, arquivos, histórico e fila. |
| `get_memory {terminalId?}` | Memória, alma, treinamento, perfil inicial e contexto da pessoa; o gerente pode consultar outro membro da sala. |
| `set_professional_profile {name, personality, content}` | Cria uma vez o próprio perfil profissional, preservando o nome atual; não registra aprendizado nem aprova treinamento. |
| `get_soul {terminalId?}` | Consulta a proposta e a versão aprovada da alma. O gerente pode consultar um membro da sala. |
| `set_soul {content, expectedRevision}` | Propõe a própria identidade durante treinamento; não altera a versão aprovada. |
| `set_memory {content, expectedRevision}` | Salva o aprendizado da própria pessoa, preservando atualizações concorrentes. |
| `set_training_complete {terminalId, expectedRevision, expectedSoulRevision}` | Gerente treinado aprova alma e formação de um membro após revisar alma e memória e concluir a conversa. O usuário aprova o gerente pela interface. |

O usuário contrata e retira pessoas pela interface. O gerente atualiza a tarefa compartilhada por `set_task`; os demais membros atualizam apenas sua própria atividade. A identidade MCP de cada pessoa é vinculada à sua sessão. Perguntas e aprovações do Codex aparecem em **Equipe**. Interações de plugins que exigem outros tipos de solicitação do App Server ainda não são suportadas e retornam um erro ao agente, em vez de deixá-lo aguardando indefinidamente.

## Instruções da Área

Ao criar uma Área, selecionar uma Área existente ou iniciar uma sessão gerenciada,
o aplicativo prepara `<pasta-raiz>/.kanasha/AGENTS.md` com o [template padrão](.kanasha/AGENTS.md),
caso ele não exista. O template explica o MCP `kanasha_terminal`, suas doze ferramentas,
exemplos de atualização, papéis, delegação, contexto, retomada e boas práticas de entrega.
No `AGENTS.md` da raiz, adiciona uma referência ao arquivo, preservando o texto existente.
Uma referência já presente não é duplicada. Arquivos personalizados não são substituídos
por novas versões do template. Remover a Área da aplicação não remove esses arquivos.

Use **Instruções da Área**, na barra da área, para editar o Markdown pelo aplicativo,
ou edite o arquivo diretamente. O editor aceita até 64 KiB e detecta se o arquivo foi
alterado desde sua abertura, evitando sobrescrever uma edição externa conhecida.
Falhas de leitura, UTF-8 inválido e conteúdo vazio/excessivo são apresentados como erro;
uma nova sessão gerenciada não inicia silenciosamente sem suas instruções.

Nas sessões de **Equipe**, o conteúdo é passado explicitamente ao Codex como
`developerInstructions`, junto da identidade e das orientações da pessoa, tanto em
`thread/start` quanto em `thread/resume`. Na retomada, se essas instruções mudaram,
o aplicativo também acrescenta a nova versão ao contexto por `thread/inject_items`,
sem iniciar uma execução do modelo. O hash salvo evita repetir essa inclusão em cada
reabertura sem mudanças. A leitura não depende da referência no arquivo
raiz. Isso complementa a descoberta nativa dos AGENTS.md do projeto, sem substituir esses
arquivos nem configurar instruções globais do Codex. O caminho `.kanasha/AGENTS.md` é uma
convenção do aplicativo, não uma importação automática do Codex.

A tela Equipe mostra o identificador da versão carregada por cada sessão e avisa quando
há alteração pendente. A comparação ocorre nas atualizações do aplicativo e ao retornar
o foco à janela; não há monitoramento contínuo de arquivos. Salvar não interrompe trabalho:
encerre e reabra a sessão para carregar a nova versão, preservando o histórico do Codex.
Interromper apenas o trabalho não recarrega as instruções. Para trocar a pasta-raiz,
encerre os terminais da Área primeiro.

O carregamento explícito e o indicador de versão cobrem os membros gerenciados pela
Equipe. Em terminais comuns e agentes externos, a referência no AGENTS.md orienta a
leitura quando descoberta pelo cliente; o arquivo por si só não instala/configura o MCP.
Essa versão das instruções é informação de sessão, não um histórico completo de auditoria.

## Privacidade da persistência

O aplicativo persiste o resumo da tarefa, cadastro da equipe, personalidades, memórias, estado de treinamento, orientações, identificadores de sessões Codex, atribuições e resultados. O estado de conexão e atividade atual é transitório. Buffers brutos de terminal e credenciais não são salvos pelo KanashaTerminal. O Codex mantém seu próprio histórico e autenticação. O executável e os argumentos de perfis customizados continuam somente em memória e devem ser informados novamente após reiniciar o aplicativo. Encerrar a sessão não apaga o histórico do Codex.

## Resumo da tarefa via MCP

O botão **Resumo**, ao lado de **+ Terminal**, abre uma tela própria sem desmontar os terminais do Workspace. Além do status da tarefa, a tela mostra cada terminal e seu estado de agente: `Disponível`, `Trabalhando`, `Aguardando`, `Bloqueado`, `Concluído` ou `Desconectado`. A cor da aba do Workspace indica a situação agregada, priorizando `Trabalhando` quando pelo menos um agente está executando uma etapa.

Ao iniciar o perfil **Codex**, o KanashaTerminal configura automaticamente o MCP local. O catálogo de `tools/list` é resolvido pelo backend a partir do tipo salvo do workspace e da identidade autenticada do terminal; não depende de uma lista fixa do cliente.

| Tipo do workspace | Ferramentas |
| --- | --- |
| Organização normal | `get_workspace_summary`, `get_workspace_steps`, `update_workspace_summary` |
| Equipe | `get_task`, `get_task_steps`, `set_task`, `get_tasks` e ferramentas de membros, atribuições, memória, alma e entregas |

Na organização normal, as consultas não recebem argumentos. `get_workspace_summary` retorna status, resumo, andamento, próximo passo, bloqueio e revisão, sem as etapas; `get_workspace_steps` retorna etapas e revisão. `update_workspace_summary` altera somente campos enviados e exige ao menos um campo. Essas ferramentas não aceitam `taskCode` nem criam tarefas da equipe. O resumo existente é preservado.

Na equipe, `get_task`, `get_task_steps` e `set_task` mantêm os contratos atuais: `taskCode` seleciona uma tarefa; sem código, tratam a visão compartilhada da sala. `get_tasks` lista as tarefas. As permissões do gerente e dos membros continuam sendo verificadas em cada operação.

O backend rejeita chamadas a ferramentas de outro tipo mesmo que um cliente tente chamá-las diretamente. Falha de autenticação ou de identificação do workspace não libera um catálogo genérico. A listagem consulta o estado atual; não há notificação automática `listChanged`.

Os campos compartilhados do resumo são `status`, `summary`, `steps`, `stepStatus`, `current`, `next` e `blocker`. `summary` aceita Markdown com até 8.000 caracteres. `steps` substitui a lista completa (até 20 itens); preserve os IDs. Para atualizar uma etapa, prefira `stepStatus`. `agentStatus` e `agentCurrent` atualizam a atividade do terminal identificado pela sessão. Esses dados continuam visíveis em **Resumo**.

Exemplo no workspace normal: `update_workspace_summary {"summary":"Investigação concluída","stepStatus":{"id":"investigar","status":"done"}}`. Na equipe, use `set_task` com o código real quando estiver atualizando uma tarefa específica. As ferramentas de atualização do resumo respondem `ok`.

Após atualizar o aplicativo, reabra as sessões para renovar o catálogo e as instruções em cache. Os nomes antigos de tarefa deixam de ser aceitos em organizações normais; atualize instruções personalizadas para usar as ferramentas de resumo nesse tipo de workspace.

O executável também aceita `--mcp` para clientes MCP STDIO. Encaminhe `KANASHA_MCP_SOCKET`, `KANASHA_MCP_TOKEN`, `KANASHA_WORKSPACE_ID` e `KANASHA_TERMINAL_ID`. São chamadas MCP locais, não endpoints HTTP; IDs extras ou o tipo do workspace não devem ser enviados nos argumentos das ferramentas.

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
cd src-tauri && cargo fmt --check && cargo test
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

### Fila por funcionário e troca de tarefa

Cada funcionário tem uma fila de tarefas em ordem de chegada e uma fila de steps
por tarefa, com até dez atribuições pendentes no total. Executa uma por vez, mantendo
a participação atual até a conclusão global ou liberação individual pelo gerente.
Novos steps da atual passam antes das tarefas
seguintes; sem steps disponíveis, aguarda o gerente e preserva o contexto. O gerente pode enfileirar várias tarefas para a mesma pessoa enquanto ela
trabalha; quantidade de tarefas não exige igual quantidade de agentes. A tela Equipe
e `get_team`/`get_agent` mostram a fila. O vínculo tarefa–funcionário fica persistido
na atribuição (`taskCode`, `stepIds`, destinatário, `requestId`), com status e resultado próprios.
**Tarefas e responsáveis** e `get_team.tasks` permitem consultar por código Jira; uma
tarefa pode ter várias pessoas e vários steps.

Use `taskCode` com o código real do Jira. Analisar, planejar e implementar a mesma
`B2C-1234` mantêm a conversa. Ao passar para `B2C-5678`, o funcionário inicia outra
conversa, carregando identidade, alma, lembranças e orientações atuais. O gerente
mantém uma conversa por task, retomada pelo seletor do terminal. Sem código a conversa é preservada por padrão;
`freshContext: true` serve para uma demanda independente sem código. Itens antigos
continuam mantendo contexto.

**Nova conversa** e a tool `reset_context` fazem uma troca manual sem apagar alma,
memória, arquivos, histórico Codex ou fila. O gerente só pode fazê-la para outro
funcionário da sala; a interface permite ao usuário fazê-la também para o gerente.
Turno ativo, pergunta pendente, resultado incerto e pausa bloqueiam a troca. Não use
a ação ao mudar steps. Recibos persistidos evitam repetir a troca após resposta
incerta. O `/clear` direto do Codex continua sem acompanhamento de troca de thread
na aplicação; use **Nova conversa** nas sessões gerenciadas.

O gerente registra cada tarefa localmente por `set_task` com `taskCode`, `summary`
e `steps` com IDs estáveis. `get_tasks` lista as tarefas e `get_task`/`get_task_steps`
aceitam o código para consultar uma delas. Steps têm títulos livres e vínculo
estruturado via `stepIds`; não exigem código no título. A aplicação rejeita steps
inexistentes ou de outra tarefa. A tela Equipe mostra steps e responsáveis e permite
selecionar steps ao delegar. Esses registros são locais ao KanashaTerminal; não
alteram issues no Jira.

Após validar resultados, o gerente atualiza os steps e marca a tarefa
`done` por `set_task {taskCode,status:"done"}`. Só então o funcionário inicia a
próxima tarefa e troca de conversa. `done` é rejeitado enquanto houver steps não
concluídos ou atribuições queued/running/uncertain. Reabrir uma tarefa exige uma
atualização explícita; uma nova atribuição a insere novamente ao fim da fila.
Consultas e atualizações sem `taskCode` preservam a visão geral da sala existente.

## Conversas do gerente por tarefa

No terminal do gerente, o dropdown à esquerda de **Encerrar processo** seleciona **Geral** ou uma task. **+ Outra task…** aceita o código e abre a conversa correspondente; se ainda não existir, registra a task local e inicia uma conversa vazia. Não cria arquivos ou worktrees. Trocar de task guarda a sessão atual e retoma o histórico da selecionada, inclusive após reabrir o aplicativo. A conversa anterior à atualização fica em Geral.

A troca fica disponível durante turnos ativos, perguntas, resultados incertos ou pausa; só é bloqueada enquanto outra troca está sendo aplicada. O chat anterior mantém seu servidor e seu turno em segundo plano. Ao voltar, a tela reconecta à mesma sessão, com a pergunta e o resultado da conversa de origem. O dropdown indica chats em andamento ou com pergunta pendente. Selecionar uma conversa pausada a disponibiliza novamente. O botão de nova conversa reinicia somente a conversa selecionada, desde que ela esteja disponível e sem execução/pergunta pendente. Alma, memória, tarefas e histórico das outras conversas são preservados. Retornos de funcionários mantêm a task de origem e aguardam sua seleção; o dropdown mostra quantos estão pendentes. Nenhuma troca automática tira o usuário da task atual.

Retornos recebidos enquanto outra conversa está selecionada ficam persistidos na fila da task de origem, inclusive com o gerente pausado, em revisão de treinamento ou com mais de dez retornos pendentes. A conclusão do funcionário e o retorno ao gerente são gravados juntos; repetir o evento não duplica a mensagem. Reabrir o aplicativo preserva essa fila. Ao selecionar a task, o gerente processa seus retornos em ordem quando estiver disponível.

A lixeira ao lado do seletor pede confirmação para excluir a conversa selecionada do KanashaTerminal. Ao excluir uma task, retorna para Geral; abrir novamente seu código inicia outra conversa. Tasks, arquivos, memória e retornos pendentes são preservados. Chats excluídos saem da lista, mas tarefas com retornos pendentes continuam visíveis. Excluir Geral inicia uma conversa vazia. O histórico original armazenado pelo Codex não é apagado. A exclusão fica bloqueada durante execução, pausa, perguntas e resultados incertos.
