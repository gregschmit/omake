SRCS = src/net.c src/io.c lib/util.h main.c
all:
	@echo "M: ${SRCS:M*.c}"
	@echo "N: ${SRCS:N*.c}"
	@echo "S: ${SRCS:S/.c/.o/}"
	@echo "T: ${SRCS:T}"
	@echo "H: ${SRCS:H}"
	@echo "E: ${SRCS:E}"
	@echo "R: ${SRCS:R}"
	@echo "chain: ${SRCS:M*.c:T:S/.c/.o/}"
	@echo "case: ${SRCS:M*.c:T:tu}"
	@echo "Q: ${SRCS:Q}"
	@echo "U: ${UNDEF:Udefault}"
