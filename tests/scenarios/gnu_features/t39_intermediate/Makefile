%.o: %.c
	@echo compile $<
	@touch $@
%.c: %.y
	@echo yacc $<
	@touch $@
prog.o: prog.y
