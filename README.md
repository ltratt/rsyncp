# rsyncp: an ETA wrapper for rsync

`rsync` is a wonderful tool, but does not provide any notion of "how much has
been done / how much is left"? `rsyncp` does just that, providing ETAs and
percentage completed for repeated `rsync` invocations: it remembers the
previous run's elapsed time and total files and reuses those on the next run.
On the first run, when no such information is available, it prints out the
number of files `rsync` has found so far, so you know `rsync` is running.

In general, to use `rsyncp` you need only change:

```sh
rsync -arg1 -arg2 src1 src2 dst
```

to:

```sh
rsyncp -- -arg1 -arg2 src1 src2 dst
```

The arguments to the right of the `--` are used to form a "cookie" that
identifies this command. If you use multiple different `rsync` invocations,
`rsyncp` treats them distinctly, giving appropriate ETAs for each.


## Usage

`rsyncp` has the following command-line format:

```
usage: rsyncp [-p prefix] [-s suffix] [-x flag] -- [rsync_arg_1 ... rsync_arg_2] SRC... [DEST]
```

where:

* `-p` is a prefix string printed on the left-hand side of the terminal.
* `-s` is a suffix string printed on the left-hand side of the terminal.
* `-x` takes a string (including `-` and `--` prefixes) and excludes it from
  the cookie calculated from the rsync arguments to the right of the `--`. If,
  for example, you pass `--exclude-from=/random/path/to` to `rsync` and don't
  want the ever-changing random path to confuse the cookie, then pass `-x
  "--exclude-from"` to `rsyncp`. Note: this matches against both plain
  `--exclude-from` and `--exclude-from=...`.

When `rsyncp` calls `rsync` it will add `--info=progress2` as the first
argument to `rsync` automatically.


## State storage

`rsyncp` stores state in:

```
${XDG_STATE_HOME:-$HOME/.local/state}/rsyncp/
```

The state file name is a hash of the `rsync` arguments, after any `-x` flags
have been excluded. Removing the state makes `rsyncp` forget all previous runs.


## Limitations

`rsyncp` estimates progress from the number of files `rsync` reports it has to
check. This means that the ETA is only accurate when repeated invocations have
similar file counts and similar per-file checking costs.
