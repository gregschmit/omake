X = 5
Y = hello

# `&&` binds tighter than `||`; parentheses override; `!` negates a primary.
.if 0 && 1 || 1
A = or_over_and
.endif

.if (0 || 1) && 0
B = wrong
.else
B = paren
.endif

.if ${X} > 3 && ${X} < 10 && defined(Y)
C = chain
.endif

.if !defined(Z) || 0
D = neg_or
.endif

# No whitespace around `&&` keeps it a bare (true) string, matching bmake.
.if 1&&0
E = bare
.endif

all:
	@echo "A=$(A) B=$(B) C=$(C) D=$(D) E=$(E)"
