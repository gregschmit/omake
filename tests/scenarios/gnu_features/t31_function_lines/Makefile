$(info configuring)
URL := $(subst SP,/,http:SPSPx#frag)
NAMES := a b
join2 = $(1)-$(2)

all:
	@echo "url=[$(URL)]"
	@echo "pairs=[$(foreach n,$(NAMES),$(call join2,$(n),z))]"
	@echo "shell=[$(shell echo 'has#hash')]"
