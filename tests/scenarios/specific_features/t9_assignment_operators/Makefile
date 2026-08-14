A := immediate
B = $(A)-deferred
A := changed
C ?= first
C ?= second
CC ?= shouldnotwin
D = base
D += more
E := x
E += y
F != echo hello from shell
all:
	@echo B=$(B)
	@echo C=$(C)
	@echo CC=$(CC)
	@echo D=$(D)
	@echo E=$(E)
	@echo F=$(F)
