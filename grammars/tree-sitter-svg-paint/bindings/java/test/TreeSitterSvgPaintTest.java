import io.github.treesitter.jtreesitter.Language;
import io.github.treesitter.jtreesitter.svgpaint.TreeSitterSvgPaint;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertDoesNotThrow;

public class TreeSitterSvgPaintTest {
    @Test
    public void testCanLoadLanguage() {
        assertDoesNotThrow(() -> new Language(TreeSitterSvgPaint.language()));
    }
}
