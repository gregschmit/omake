NAMES = alpha beta gamma
greet = hello-$(1)
FLAG = yes

all:
	@echo "loop=[$(foreach n,$(NAMES),$(call greet,$(n)))]"
	@echo "pick=[$(if $(FLAG),on,off)] [$(if $(MISSING),on,off)]"
	@echo "shell=[$(shell echo abc | tr a-c ABC)]"
	@echo "or=[$(or $(MISSING),,fallback)] and=[$(and a,b,z)]"
	@echo "words=[$(foreach w,$(NAMES),$(w):$(words $(NAMES)))]"
