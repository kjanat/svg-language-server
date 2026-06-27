import io.github.treesitter.jtreesitter.Language;
import io.github.treesitter.jtreesitter.svg.TreeSitterSvg;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertDoesNotThrow;

public class TreeSitterSvgTest {
    @Test
    public void testCanLoadLanguage() {
        assertDoesNotThrow(() -> new Language(TreeSitterSvg.language()));
    }
}
