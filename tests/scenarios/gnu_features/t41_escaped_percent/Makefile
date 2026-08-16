V = a%x b%y cx
all:
	@echo "subref: $(V:a\%x=HIT)"
	@echo "patsubst: $(patsubst a\%x,HIT,$(V))"
	@echo "filter: $(filter a\%x,$(V))"
	@echo "wildcard: $(patsubst x\%%,S-%,x%a x%b y)"
