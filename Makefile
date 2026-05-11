PREFIX ?= /usr/local
MAN_PREFIX ?= ${PREFIX}/man

all:
# Intentionally empty


install:
	install -d ${PREFIX}/bin
	install -c -m 555 rsyncp ${PREFIX}/bin
	install -d ${MAN_PREFIX}/man1
	install -c -m 444 rsyncp.1 ${MAN_PREFIX}/man1/rsyncp.1


clean:
# Intentionally empty


distrib:
	@read v?'rsyncp version: '; mkdir rsyncp-$$v; \
	cp CHANGES.md rsyncp rsyncp.1 COPYRIGHT LICENSE-* Makefile rsyncp-$$v; \
	  tar cfz rsyncp-$$v.tgz rsyncp-$$v; \
	  rm -rf rsyncp-$$v
