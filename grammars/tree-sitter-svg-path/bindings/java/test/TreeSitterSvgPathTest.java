import io.github.treesitter.jtreesitter.Language;
import io.github.treesitter.jtreesitter.svgpath.TreeSitterSvgPath;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertDoesNotThrow;

public class TreeSitterSvgPathTest {
    @Test
    public void testCanLoadLanguage() {
        assertDoesNotThrow(() -> new Language(TreeSitterSvgPath.language()));
    }
}
