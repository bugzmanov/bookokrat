1 //_These sectors are for 2048-byte sectors_.
//_Multiply by 4 for devices with 512-byte sectors_.
3 if(cur_cmd.sector>=10000 && cur_cmd.sector<48000)
tamperdetected=true;
5
//_This is the legitimate read_.
7 cur_cmd.last_result = storage_read_sectors(
IF_MD2(cur_cmd.lun,) cur_cmd.sector,
9 MIN(READ_BUFFER_SIZE/SECTOR_SIZE, cur_cmd.count),
cur_cmd.data[cur_cmd.data_select]
11 );
13 //_Here, we wipe the buffer to demo antiforensics_.
if(tamperdetected){
15 for(i=0;i<READ_BUFFER_SIZE;i++)
cur_cmd.data[cur_cmd.data_select][i]=0xFF;
17 //_Clobber the buffer for testing_.
strcpy(cur_cmd.data[cur_cmd.data_select],
19 "Never gonna let you down.");
21 //_Comment the following to make a harmless demo_.
//_This writes the buffer back to the disk_,
23 //_eliminating any of the old contents_.
if(cur_cmd.sector>=48195)
25 storage_write_sectors(
IF_MD2(cur_cmd.lun,)
27 cur_cmd.sector,
MIN(WRITE_BUFFER_SIZE/SECTOR_SIZE, cur_cmd.count),
29 cur_cmd.data[cur_cmd.data_select]);
}
