import io.github.treesitter.jtreesitter.Language;
import io.github.treesitter.jtreesitter.svgtransform.TreeSitterSvgTransform;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertDoesNotThrow;

public class TreeSitterSvgTransformTest {
    @Test
    public void testCanLoadLanguage() {
        assertDoesNotThrow(() -> new Language(TreeSitterSvgTransform.language()));
    }
}
