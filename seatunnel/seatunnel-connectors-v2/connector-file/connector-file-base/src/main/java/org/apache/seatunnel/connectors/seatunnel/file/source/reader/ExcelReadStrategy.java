/*
 * Licensed to the Apache Software Foundation (ASF) under one or more
 * contributor license agreements.  See the NOTICE file distributed with
 * this work for additional information regarding copyright ownership.
 * The ASF licenses this file to You under the Apache License, Version 2.0
 * (the "License"); you may not use this file except in compliance with
 * the License.  You may obtain a copy of the License at
 *
 *    http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

package org.apache.seatunnel.connectors.seatunnel.file.source.reader;

import org.apache.seatunnel.api.source.Collector;
import org.apache.seatunnel.api.table.catalog.CatalogTable;
import org.apache.seatunnel.api.table.type.SeaTunnelDataType;
import org.apache.seatunnel.api.table.type.SeaTunnelRow;
import org.apache.seatunnel.api.table.type.SeaTunnelRowType;
import org.apache.seatunnel.common.exception.CommonErrorCodeDeprecated;
import org.apache.seatunnel.common.utils.DateTimeUtils;
import org.apache.seatunnel.common.utils.DateUtils;
import org.apache.seatunnel.common.utils.TimeUtils;
import org.apache.seatunnel.connectors.seatunnel.file.config.ExcelEngine;
import org.apache.seatunnel.connectors.seatunnel.file.config.FileBaseSourceOptions;
import org.apache.seatunnel.connectors.seatunnel.file.config.FileFormat;
import org.apache.seatunnel.connectors.seatunnel.file.excel.ExcelCellUtils;
import org.apache.seatunnel.connectors.seatunnel.file.excel.ExcelReaderListener;
import org.apache.seatunnel.connectors.seatunnel.file.exception.FileConnectorException;

import org.apache.poi.hssf.usermodel.HSSFWorkbook;
import org.apache.poi.openxml4j.exceptions.OpenXML4JException;
import org.apache.poi.openxml4j.opc.OPCPackage;
import org.apache.poi.ss.usermodel.Cell;
import org.apache.poi.ss.usermodel.CellStyle;
import org.apache.poi.ss.usermodel.CellType;
import org.apache.poi.ss.usermodel.CellValue;
import org.apache.poi.ss.usermodel.DataFormatter;
import org.apache.poi.ss.usermodel.DateUtil;
import org.apache.poi.ss.usermodel.FormulaEvaluator;
import org.apache.poi.ss.usermodel.Sheet;
import org.apache.poi.ss.usermodel.Workbook;
import org.apache.poi.ss.util.CellReference;
import org.apache.poi.ss.util.NumberToTextConverter;
import org.apache.poi.util.XMLHelper;
import org.apache.poi.xssf.eventusermodel.XSSFReader;
import org.apache.poi.xssf.model.SharedStringsTable;
import org.apache.poi.xssf.model.StylesTable;

import org.xml.sax.Attributes;
import org.xml.sax.InputSource;
import org.xml.sax.SAXException;
import org.xml.sax.XMLReader;
import org.xml.sax.helpers.DefaultHandler;

import com.alibaba.excel.EasyExcel;
import com.alibaba.excel.read.builder.ExcelReaderBuilder;
import lombok.Getter;
import lombok.SneakyThrows;
import lombok.extern.slf4j.Slf4j;

import javax.xml.parsers.ParserConfigurationException;

import java.io.IOException;
import java.io.InputStream;
import java.time.LocalDateTime;
import java.time.format.DateTimeFormatter;
import java.util.ArrayList;
import java.util.Iterator;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.stream.IntStream;

@Getter
@Slf4j
public class ExcelReadStrategy extends AbstractReadStrategy {

    private String dateFormatterPattern = DateUtils.Formatter.YYYY_MM_DD.getValue();

    private String dateTimeFormatterPattern =
            DateTimeUtils.Formatter.YYYY_MM_DD_HH_MM_SS.getValue();

    private String timeFormatterPattern = TimeUtils.Formatter.HH_MM_SS.getValue();

    private int[] indexes;

    private int cellCount;

    @SneakyThrows
    @Override
    public void read(String path, String tableId, Collector<SeaTunnelRow> output) {
        Map<String, String> partitionsMap = parsePartitionsByPath(path);
        resolveArchiveCompressedInputStream(path, tableId, output, partitionsMap, FileFormat.EXCEL);
    }

    @Override
    protected void readProcess(
            String path,
            String tableId,
            Collector<SeaTunnelRow> output,
            InputStream inputStream,
            Map<String, String> partitionsMap,
            String currentFileName)
            throws IOException {

        if (skipHeaderNumber > Integer.MAX_VALUE || skipHeaderNumber < Integer.MIN_VALUE) {
            throw new FileConnectorException(
                    CommonErrorCodeDeprecated.UNSUPPORTED_OPERATION,
                    "Skip the number of rows exceeds the maximum or minimum limit of Sheet");
        }

        if (pluginConfig.hasPath(FileBaseSourceOptions.DATE_FORMAT_LEGACY.key())) {
            dateFormatterPattern =
                    pluginConfig.getString(FileBaseSourceOptions.DATE_FORMAT_LEGACY.key());
        }
        if (pluginConfig.hasPath(FileBaseSourceOptions.DATETIME_FORMAT_LEGACY.key())) {
            dateTimeFormatterPattern =
                    pluginConfig.getString(FileBaseSourceOptions.DATETIME_FORMAT_LEGACY.key());
        }
        if (pluginConfig.hasPath(FileBaseSourceOptions.TIME_FORMAT_LEGACY.key())) {
            timeFormatterPattern =
                    pluginConfig.getString(FileBaseSourceOptions.TIME_FORMAT_LEGACY.key());
        }

        ExcelCellUtils excelCellUtils =
                new ExcelCellUtils(
                        pluginConfig,
                        dateFormatterPattern,
                        dateTimeFormatterPattern,
                        timeFormatterPattern);

        if (pluginConfig.hasPath(FileBaseSourceOptions.EXCEL_ENGINE.key())
                && pluginConfig
                        .getString(FileBaseSourceOptions.EXCEL_ENGINE.key())
                        .equals(ExcelEngine.EASY_EXCEL.getExcelEngineName())) {
            log.info("Parsing Excel with EasyExcel");

            ExcelReaderBuilder read =
                    EasyExcel.read(
                            inputStream,
                            new ExcelReaderListener(
                                    tableId, output, excelCellUtils, seaTunnelRowType));
            if (pluginConfig.hasPath(FileBaseSourceOptions.SHEET_NAME.key())) {
                // Use doRead instead of doReadSync. doReadSync collects all rows into an
                // in-memory list internally, which will cause OOM for large excel files.
                read.sheet(pluginConfig.getString(FileBaseSourceOptions.SHEET_NAME.key()))
                        .headRowNumber((int) skipHeaderNumber)
                        .doRead();
            } else {
                read.sheet(0).headRowNumber((int) skipHeaderNumber).doRead();
            }
        } else {
            log.info("Parsing Excel with POI");
            cellCount = seaTunnelRowType.getTotalFields();
            cellCount = partitionsMap.isEmpty() ? cellCount : cellCount + partitionsMap.size();
            if (currentFileName.endsWith(".xls")) {
                readXls(
                        inputStream,
                        tableId,
                        output,
                        partitionsMap,
                        excelCellUtils,
                        currentFileName);
            } else if (currentFileName.endsWith(".xlsx")) {
                // Parse xlsx in a streaming way with XSSFReader and SAX, which only keeps the
                // current row in memory. The previous XSSFWorkbook approach loaded the whole
                // workbook (all rows, cells and styles) into the heap and caused OOM for large
                // excel files.
                readXlsx(
                        inputStream,
                        tableId,
                        output,
                        partitionsMap,
                        excelCellUtils,
                        currentFileName);
            } else {
                throw new FileConnectorException(
                        CommonErrorCodeDeprecated.UNSUPPORTED_OPERATION,
                        "Only support read excel file");
            }
        }
    }

    @Override
    public void setCatalogTable(CatalogTable catalogTable) {
        SeaTunnelRowType rowType = catalogTable.getSeaTunnelRowType();
        if (isNullOrEmpty(rowType.getFieldNames()) || isNullOrEmpty(rowType.getFieldTypes())) {
            throw new FileConnectorException(
                    CommonErrorCodeDeprecated.UNSUPPORTED_OPERATION,
                    "Schema information is not set or incorrect Schema settings");
        }
        SeaTunnelRowType userDefinedRowTypeWithPartition =
                mergePartitionTypes(fileNames.get(0), rowType);
        // column projection
        if (pluginConfig.hasPath(FileBaseSourceOptions.READ_COLUMNS.key())) {
            // get the read column index from user-defined row type
            indexes = new int[readColumns.size()];
            String[] fields = new String[readColumns.size()];
            SeaTunnelDataType<?>[] types = new SeaTunnelDataType[readColumns.size()];
            for (int i = 0; i < indexes.length; i++) {
                indexes[i] = rowType.indexOf(readColumns.get(i));
                fields[i] = rowType.getFieldName(indexes[i]);
                types[i] = rowType.getFieldType(indexes[i]);
            }
            this.seaTunnelRowType = new SeaTunnelRowType(fields, types);
            this.seaTunnelRowTypeWithPartition =
                    mergePartitionTypes(fileNames.get(0), this.seaTunnelRowType);
        } else {
            this.seaTunnelRowType = rowType;
            this.seaTunnelRowTypeWithPartition = userDefinedRowTypeWithPartition;
        }
    }

    @Override
    public SeaTunnelRowType getSeaTunnelRowTypeInfo(String path) throws FileConnectorException {
        throw new FileConnectorException(
                CommonErrorCodeDeprecated.UNSUPPORTED_OPERATION,
                "User must defined schema for json file type");
    }

    private void readXls(
            InputStream inputStream,
            String tableId,
            Collector<SeaTunnelRow> output,
            Map<String, String> partitionsMap,
            ExcelCellUtils excelCellUtils,
            String currentFileName)
            throws IOException {
        try (Workbook workbook = new HSSFWorkbook(inputStream)) {
            FormulaEvaluator formulaEvaluator =
                    workbook.getCreationHelper().createFormulaEvaluator();
            DataFormatter formatter = new DataFormatter();
            Sheet sheet =
                    pluginConfig.hasPath(FileBaseSourceOptions.SHEET_NAME.key())
                            ? workbook.getSheet(
                                    pluginConfig.getString(FileBaseSourceOptions.SHEET_NAME.key()))
                            : workbook.getSheetAt(0);
            SeaTunnelDataType<?>[] fieldTypes = seaTunnelRowType.getFieldTypes();
            int rowCount = sheet.getPhysicalNumberOfRows();
            if (skipHeaderNumber > rowCount) {
                throw new FileConnectorException(
                        CommonErrorCodeDeprecated.UNSUPPORTED_OPERATION,
                        "Skip the number of rows exceeds the maximum or minimum limit of Sheet");
            }
            IntStream.range((int) skipHeaderNumber, rowCount)
                    .mapToObj(sheet::getRow)
                    .filter(Objects::nonNull)
                    .forEach(
                            rowData -> {
                                SeaTunnelRow seaTunnelRow = new SeaTunnelRow(cellCount);
                                int z = 0;
                                for (int j : getCellIndexes()) {
                                    Cell cell = rowData.getCell(j);
                                    seaTunnelRow.setField(
                                            z++,
                                            cell == null
                                                    ? null
                                                    : excelCellUtils.convert(
                                                            getCellValue(
                                                                    cell.getCellType(),
                                                                    cell,
                                                                    formulaEvaluator,
                                                                    formatter),
                                                            fieldTypes[z - 1],
                                                            null));
                                }
                                fillPartitionFields(seaTunnelRow, partitionsMap);
                                seaTunnelRow.setTableId(tableId);
                                output.collect(seaTunnelRow);
                            });
        }
    }

    private void readXlsx(
            InputStream inputStream,
            String tableId,
            Collector<SeaTunnelRow> output,
            Map<String, String> partitionsMap,
            ExcelCellUtils excelCellUtils,
            String currentFileName)
            throws IOException {
        try (OPCPackage pkg = OPCPackage.open(inputStream)) {
            XSSFReader reader = new XSSFReader(pkg);
            SharedStringsTable sharedStringsTable = reader.getSharedStringsTable();
            StylesTable stylesTable = reader.getStylesTable();
            ExcelSheetContentsHandler sheetContentsHandler =
                    new ExcelSheetContentsHandler(
                            tableId,
                            output,
                            partitionsMap,
                            sharedStringsTable,
                            stylesTable,
                            excelCellUtils);
            XMLReader xmlReader = XMLHelper.newXMLReader();
            xmlReader.setContentHandler(sheetContentsHandler);
            if (pluginConfig.hasPath(FileBaseSourceOptions.SHEET_NAME.key())) {
                String sheetName = pluginConfig.getString(FileBaseSourceOptions.SHEET_NAME.key());
                InputStream sheetStream = getSheetStreamByName(reader, sheetName);
                if (sheetStream == null) {
                    throw new FileConnectorException(
                            CommonErrorCodeDeprecated.UNSUPPORTED_OPERATION,
                            "Sheet [" + sheetName + "] was not found in the excel file");
                }
                try (InputStream stream = sheetStream) {
                    xmlReader.parse(new InputSource(stream));
                }
            } else {
                Iterator<InputStream> sheets = reader.getSheetsData();
                if (!sheets.hasNext()) {
                    throw new FileConnectorException(
                            CommonErrorCodeDeprecated.UNSUPPORTED_OPERATION,
                            "The excel file does not contain any sheet");
                }
                try (InputStream stream = sheets.next()) {
                    xmlReader.parse(new InputSource(stream));
                }
            }
        } catch (OpenXML4JException | SAXException | ParserConfigurationException e) {
            throw new IOException("Failed to parse excel file: " + currentFileName, e);
        }
    }

    private InputStream getSheetStreamByName(XSSFReader reader, String sheetName)
            throws IOException, OpenXML4JException {
        Iterator<InputStream> sheets = reader.getSheetsData();
        while (sheets.hasNext()) {
            // The sheet name is only available after next() is called, since it is lazily
            // loaded by the SheetIterator while advancing to the next sheet.
            InputStream stream = sheets.next();
            if (sheetName.equals(((XSSFReader.SheetIterator) sheets).getSheetName())) {
                return stream;
            }
            stream.close();
        }
        return null;
    }

    private int[] getCellIndexes() {
        return indexes == null ? IntStream.range(0, cellCount).toArray() : indexes;
    }

    private void fillPartitionFields(SeaTunnelRow seaTunnelRow, Map<String, String> partitionsMap) {
        if (isMergePartition) {
            int index = seaTunnelRowType.getTotalFields();
            for (String value : partitionsMap.values()) {
                seaTunnelRow.setField(index++, value);
            }
        }
    }

    private Object getCellValue(
            CellType cellType,
            Cell cell,
            FormulaEvaluator formulaEvaluator,
            DataFormatter formatter) {
        switch (cellType) {
            case STRING:
                return cell.getStringCellValue();
            case BOOLEAN:
                return cell.getBooleanCellValue();
            case NUMERIC:
                if (DateUtil.isCellDateFormatted(cell)) {
                    return cell.getLocalDateTimeCellValue();
                }
                return formatter.formatCellValue(cell);
            case BLANK:
                return "";
            case ERROR:
                break;
            case FORMULA:
                CellValue evaluate = formulaEvaluator.evaluate(cell);
                if (evaluate.getCellType().equals(CellType.NUMERIC)) {
                    return NumberToTextConverter.toText(evaluate.getNumberValue());
                } else {
                    return evaluate.formatAsString();
                }
            default:
                throw new FileConnectorException(
                        CommonErrorCodeDeprecated.UNSUPPORTED_DATA_TYPE,
                        String.format("[%s] type not support ", cellType));
        }
        return null;
    }

    private <T> boolean isNullOrEmpty(T[] arr) {
        return arr == null || arr.length == 0;
    }

    /**
     * A streaming SAX handler used to parse the sheet xml of xlsx files. It only keeps the current
     * row in memory and collects each parsed row to the output immediately, so the memory usage
     * stays low even for huge excel files.
     */
    private class ExcelSheetContentsHandler extends DefaultHandler {

        private final String tableId;
        private final Collector<SeaTunnelRow> output;
        private final Map<String, String> partitionsMap;
        private final SharedStringsTable sharedStringsTable;
        private final StylesTable stylesTable;
        private final ExcelCellUtils excelCellUtils;
        private final SeaTunnelDataType<?>[] fieldTypes = seaTunnelRowType.getFieldTypes();
        private final DataFormatter dataFormatter = new DataFormatter();
        private final DateTimeFormatter dateFormatter =
                DateTimeFormatter.ofPattern(dateFormatterPattern);
        private final DateTimeFormatter dateTimeFormatter =
                DateTimeFormatter.ofPattern(dateTimeFormatterPattern);
        private final DateTimeFormatter timeFormatter =
                DateTimeFormatter.ofPattern(timeFormatterPattern);

        private StringBuilder cellValue;
        private String cellRef;
        private String cellType;
        private int styleIndex;
        private boolean inCell;
        private boolean inValue;
        private int rowIndex;
        private List<String> rowValues;

        ExcelSheetContentsHandler(
                String tableId,
                Collector<SeaTunnelRow> output,
                Map<String, String> partitionsMap,
                SharedStringsTable sharedStringsTable,
                StylesTable stylesTable,
                ExcelCellUtils excelCellUtils) {
            this.tableId = tableId;
            this.output = output;
            this.partitionsMap = partitionsMap;
            this.sharedStringsTable = sharedStringsTable;
            this.stylesTable = stylesTable;
            this.excelCellUtils = excelCellUtils;
        }

        @Override
        public void startElement(String uri, String localName, String qName, Attributes attributes)
                throws SAXException {
            switch (qName) {
                case "row":
                    String rowNum = attributes.getValue("r");
                    rowIndex = rowNum == null ? rowIndex + 1 : Integer.parseInt(rowNum) - 1;
                    rowValues = new ArrayList<>();
                    break;
                case "c":
                    cellRef = attributes.getValue("r");
                    cellType = attributes.getValue("t");
                    String style = attributes.getValue("s");
                    styleIndex = style == null ? 0 : Integer.parseInt(style);
                    cellValue = new StringBuilder();
                    inCell = true;
                    inValue = false;
                    break;
                case "v":
                    inValue = true;
                    break;
                case "t":
                    if (inCell) {
                        inValue = true;
                    }
                    break;
                default:
                    break;
            }
        }

        @Override
        public void characters(char[] ch, int start, int length) throws SAXException {
            if (inValue && cellValue != null) {
                cellValue.append(ch, start, length);
            }
        }

        @Override
        public void endElement(String uri, String localName, String qName) throws SAXException {
            switch (qName) {
                case "v":
                    inValue = false;
                    break;
                case "t":
                    if (inCell) {
                        inValue = false;
                    }
                    break;
                case "c":
                    handleCell();
                    inCell = false;
                    cellValue = null;
                    break;
                case "row":
                    handleRow();
                    rowValues = null;
                    break;
                default:
                    break;
            }
        }

        private void handleCell() {
            if (rowValues == null || cellValue == null) {
                return;
            }
            int colIndex = cellRef == null ? rowValues.size() : new CellReference(cellRef).getCol();
            while (rowValues.size() <= colIndex) {
                rowValues.add(null);
            }
            String raw = cellValue.toString();
            if ("s".equals(cellType)) {
                rowValues.set(
                        colIndex, sharedStringsTable.getItemAt(Integer.parseInt(raw)).getString());
                return;
            }
            if ("inlineStr".equals(cellType) || "str".equals(cellType)) {
                rowValues.set(colIndex, raw);
                return;
            }
            if ("b".equals(cellType)) {
                rowValues.set(colIndex, "1".equals(raw) ? "true" : "false");
                return;
            }
            if (raw.isEmpty()) {
                rowValues.set(colIndex, "");
                return;
            }
            double value = Double.parseDouble(raw);
            CellStyle style = stylesTable.getStyleAt(styleIndex);
            String formatString = style == null ? null : style.getDataFormatString();
            int formatIndex = style == null ? 0 : style.getDataFormat();
            if (formatString != null && DateUtil.isADateFormat(formatIndex, formatString)) {
                LocalDateTime dateTime = DateUtil.getLocalDateTime(value);
                rowValues.set(colIndex, formatDateCell(dateTime, colIndex));
            } else {
                rowValues.set(
                        colIndex,
                        dataFormatter.formatRawCellContents(value, formatIndex, formatString));
            }
        }

        private String formatDateCell(LocalDateTime dateTime, int colIndex) {
            SeaTunnelDataType<?> fieldType =
                    colIndex < fieldTypes.length ? fieldTypes[colIndex] : null;
            if (fieldType == null) {
                return dateTime.toString();
            }
            switch (fieldType.getSqlType()) {
                case DATE:
                    return dateTime.toLocalDate().format(dateFormatter);
                case TIME:
                    return dateTime.toLocalTime().format(timeFormatter);
                case TIMESTAMP:
                    return dateTime.format(dateTimeFormatter);
                default:
                    return dateTime.toString();
            }
        }

        private void handleRow() {
            if (rowValues == null || rowIndex < skipHeaderNumber) {
                return;
            }
            SeaTunnelRow seaTunnelRow = new SeaTunnelRow(cellCount);
            int z = 0;
            for (int j : getCellIndexes()) {
                String value = j < rowValues.size() ? rowValues.get(j) : null;
                seaTunnelRow.setField(
                        z++,
                        value == null
                                ? null
                                : excelCellUtils.convert(value, fieldTypes[z - 1], null));
            }
            fillPartitionFields(seaTunnelRow, partitionsMap);
            seaTunnelRow.setTableId(tableId);
            output.collect(seaTunnelRow);
        }
    }
}
