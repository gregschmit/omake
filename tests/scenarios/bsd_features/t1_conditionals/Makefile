X = 5
Y = hello

.if ${X} == 5
A = num_eq
.endif

.if ${X} != 3
B = num_ne
.endif

.if !defined(NOPE)
C = negated
.endif

.if ${Y}
D = truthy
.endif

.if 0
E = should_not
.else
E = zero_false
.endif

.ifdef X
.  ifdef Y
F = nested
.  endif
.endif

.ifndef X
G = wrong
.elif defined(Y)
G = elif_branch
.else
G = wrong
.endif

# Numeric comparison: `05` equals `5` numerically, but quoted strings differ.
.if 05 == 5
H = num
.endif
.if "05" != "5"
I = str
.endif

# Relational operators are numeric; hex literals are recognized.
.if ${X} >= 5
J = rel
.endif
.if 0x10 == 16
K = hex
.endif

all:
	@echo "A=$(A) B=$(B) C=$(C) D=$(D) E=$(E) F=$(F) G=$(G) H=$(H) I=$(I) J=$(J) K=$(K)"
