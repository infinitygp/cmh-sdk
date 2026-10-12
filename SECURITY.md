# Security

Only the current `main` branch is maintained.

Report a vulnerability privately: open the repository's **Security** tab on
GitHub and choose **Report a vulnerability**. Please do not open a public
issue for it.

Of particular interest:

- any way to make the public API send a report outside the verified command
  set described in
  [docs/protocol/README.md](docs/protocol/README.md#commands-that-must-never-be-sent)
- faults in decoding reports, fixture files or pictures that come from an
  untrusted source
