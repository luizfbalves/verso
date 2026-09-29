# Verso

[English](README.md) | Português

App gratuito para macOS e Windows que mostra a letra sincronizada da música que toca no Spotify, flutuando sobre qualquer janela.

Site e downloads: https://luizfbalves.github.io/verso/ (English: https://luizfbalves.github.io/verso/en/)

## Desenvolvimento

Requer Node 22 e Rust estável.

```sh
npm ci
npm run tauri dev     # roda o app em modo dev
npm run tauri build   # gera os instaladores em src-tauri/target/release/bundle
```

Releases saem pelo workflow [release.yml](.github/workflows/release.yml) ao publicar uma tag `v*`.

## Política de assinatura de código

Assinatura de código gratuita fornecida pela [SignPath.io](https://about.signpath.io), certificado da [SignPath Foundation](https://signpath.org).

Só os instaladores do Windows (`.exe` e `.msi`) anexados às [GitHub Releases](https://github.com/luizfbalves/verso/releases) são assinados. Eles são gerados a partir deste repositório pelo GitHub Actions e assinados apenas após aprovação manual de cada release.

Papéis da equipe:

- Committers e revisores: [@luizfbalves](https://github.com/luizfbalves)
- Aprovadores: [@luizfbalves](https://github.com/luizfbalves)

## Política de privacidade

Versão completa: https://luizfbalves.github.io/verso/privacidade.html (English: https://luizfbalves.github.io/verso/en/privacy.html)

Este programa não transfere nenhuma informação para outros sistemas em rede, a menos que isso seja solicitado especificamente pelo usuário ou por quem o instala ou opera.

Para funcionar, o Verso faz as seguintes requisições:

- **LRCLIB** (https://lrclib.net): título, artista, álbum e duração da faixa tocando no Spotify, para buscar a letra sincronizada.
- **Servidor de tradução do Verso** ([proxy/](proxy/), um Cloudflare Worker): só se o usuário ativar a tradução. As linhas da letra são repassadas ao Microsoft Azure Translator e o resultado fica num cache compartilhado sem identificadores de usuário. O IP do cliente é usado só para limitar a taxa de uso, com hash, em contadores que expiram em até dois dias.

Nenhum dado de analytics ou telemetria é coletado.

## Licença

[MIT](LICENSE)
