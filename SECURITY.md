# Security policy

This page states what `reposerve` protects and what it does not. Read it before
you serve a directory that holds private material.

## What the server protects against

- **A request outside the file list.** The server answers only a path it lists.
  Every other path is a 404, including a traversal such as `../../etc/passwd`.
- **A symlink that leaves the root.** A listed link is followed only when its
  target stays inside the root. Otherwise the request is a 404.
- **A script in a notebook.** Notebook HTML output is cleaned before it reaches
  the page. Scripts, event handlers and remote resources are removed.
- **A page that loads a third-party resource.** Every page carries a content
  security policy that allows this server alone.

## What the server does not protect against

- **Anyone who can reach the port.** Every file of an enabled type is readable
  without a token until you pass `--token`. Enable a type only for a repository
  you are willing to share on that network.
- **A hostile network.** The server speaks plain HTTP. It does not terminate
  TLS, so a token and a page cross the network in clear text. Put it behind a
  proxy when the network is not trusted.
- **Heavy or repeated requests.** The server has no rate limit and no connection
  cap. A busy network can slow it down.
- **A file that changes under the reader.** The server reads a file again when it
  changes. It does not check that the content is safe to publish.

> **Warning:** The default bind address is `0.0.0.0`, so the server answers on
> every interface. Use `--bind 127.0.0.1` to keep it on this machine.

## Supported versions

The latest release. An older version gets a fix only when that fix is cheap.

## Report a vulnerability

Open an issue. Use a private security advisory only when the detail is
sensitive. A public report helps: anyone else running `reposerve` can then fix
their copy or stop running it.

This is a hobby project. A report may go unanswered, and no response time is
promised.
