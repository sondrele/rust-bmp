RC = rustc
LIB = src/bmp.rs
MAIN = main.rs

EXEC = $(MAIN:.rs=)
TEXEC = $(LIB:.rs=)

lib: $(LIB)
	$(RC) --crate-type=lib $^

test:
	$(RC) $(LIB) --test -o $(TEXEC)
	./$(TEXEC)

clean:
	rm -f $(EXEC)
	rm -f $(TEXEC)
	rm -f *.rlib
