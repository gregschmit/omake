FROMFILE = yes

define rule_template
$(1)_built := from-$(1)
endef

$(foreach t,alpha beta,$(eval $(call rule_template,$(t))))

all:
	@echo "built=[$(alpha_built)] [$(beta_built)]"
	@echo "origins=[$(origin FROMFILE)] [$(origin CC)] [$(origin NOPE)] [$(origin @)]"
